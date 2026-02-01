# Thinking Transformer Architecture

## Исследование существующих решений

### Проанализированные проекты

| Проект | Язык | Паттерн | Применимость |
|--------|------|---------|--------------|
| [claude-code-mux](https://github.com/9j/claude-code-mux) | Rust | Provider abstraction + config-driven | Высокая |
| [llm-edge-agent](https://github.com/globalbusinessadvisors/llm-edge-agent) | Rust | Layered middleware (Axum) | Высокая |
| [nexus](https://github.com/grafbase/nexus) | Rust | Configuration composition | Средняя |
| [tower-llm](https://docs.rs/tower-llm) | Rust | Tower Service/Layer + Codec | Высокая |
| [kairos-rs](https://github.com/DanielSarmiento04/kairos-rs) | Rust | Per-route transformation | Средняя |

### Ключевые паттерны из исследования

**1. Tower Service/Layer (tower-llm)**
- Использует `Arc<dyn Transformer>` для динамической загрузки плагинов
- Codec layer для bidirectional message conversion
- Policy-driven control flow через `CompositePolicy`

**2. Axum Body Transformation Pattern**
```rust
// Decompose → Buffer → Transform → Reconstruct
let (parts, body) = req.into_parts();
let bytes = body.collect().await?.to_bytes();
// ... transform bytes ...
let req = Request::from_parts(parts, Body::from(transformed_bytes));
```

**3. Chain of Responsibility (refactoring.guru)**
- Dynamic dispatch с `Box<dyn Handler>`
- `execute()` + `handle()` + `next()` методы
- Runtime chain construction

**4. async_trait для async + trait objects**
```rust
#[async_trait]
pub trait Handler: Send + Sync {
    async fn handle(&self, request: &mut Request) -> Result<()>;
}
// Можно использовать как Box<dyn Handler>
```

### Выводы для нашего проекта

1. **Trait-based Strategy** — оптимальный выбор для нашего случая:
   - Каждый режим имеет разное поведение
   - Summarize требует async (HTTP к внешнему API)
   - Просто тестировать изолированно
   - Легко добавлять новые режимы

2. **async_trait** — необходим для async методов в trait objects

3. **Factory/Registry** — для горячей замены трансформера при изменении конфига

---

## Текущее состояние

```
ThinkingTracker
    ├── mode: ThinkingMode (enum)
    └── transform_request() -> всё в одном методе с match
```

Проблемы:
- Вся логика в одном методе с `match`
- Нельзя добавить async операции (нужно для `summarize`)
- Сложно тестировать отдельные режимы
- Нет общего интерфейса для расширения

## Новая архитектура

### Core Trait

```rust
/// Трансформер для обработки thinking блоков в запросах.
///
/// Каждый режим работы реализует этот trait.
#[async_trait]
pub trait ThinkingTransformer: Send + Sync {
    /// Имя трансформера для логирования
    fn name(&self) -> &'static str;

    /// Трансформировать запрос перед отправкой upstream.
    ///
    /// Вызывается на КАЖДЫЙ запрос к API.
    async fn transform_request(
        &self,
        body: &mut serde_json::Value,
        context: &TransformContext,
    ) -> Result<TransformResult, TransformError>;

    /// Вызывается при переключении backend (опционально).
    ///
    /// Только `summarize` режим использует это для суммаризации.
    async fn on_backend_switch(
        &self,
        _from: &str,
        _to: &str,
        _body: &mut serde_json::Value,
    ) -> Result<(), TransformError> {
        Ok(()) // По умолчанию - ничего не делаем
    }
}
```

### Context и Result

```rust
/// Контекст для трансформации
pub struct TransformContext {
    /// Текущий backend
    pub backend: String,
    /// ID запроса для трейсинга
    pub request_id: String,
}

/// Результат трансформации
#[derive(Debug, Default)]
pub struct TransformResult {
    /// Было ли изменено тело запроса
    pub changed: bool,
    /// Статистика по операциям
    pub stats: TransformStats,
}

#[derive(Debug, Default)]
pub struct TransformStats {
    pub stripped_count: u32,
    pub summarized_count: u32,
    pub passed_through_count: u32,
}

/// Ошибки трансформации
#[derive(Debug, thiserror::Error)]
pub enum TransformError {
    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Summarization failed: {0}")]
    SummarizationError(String),

    #[error("Backend not available: {0}")]
    BackendError(String),
}
```

### Реализации

```
src/proxy/thinking/
├── mod.rs              # Публичный API, TransformerRegistry
├── traits.rs           # ThinkingTransformer trait
├── context.rs          # TransformContext, TransformResult
├── error.rs            # TransformError
├── strip.rs            # StripTransformer
├── summarize.rs        # SummarizeTransformer (будущее)
└── native.rs           # NativeTransformer (будущее)
```

#### StripTransformer

```rust
/// Режим strip: полностью удаляет thinking блоки.
pub struct StripTransformer;

#[async_trait]
impl ThinkingTransformer for StripTransformer {
    fn name(&self) -> &'static str { "strip" }

    async fn transform_request(
        &self,
        body: &mut Value,
        _context: &TransformContext,
    ) -> Result<TransformResult, TransformError> {
        let mut result = TransformResult::default();

        if let Some(messages) = body.get_mut("messages").and_then(|v| v.as_array_mut()) {
            for message in messages {
                if let Some(content) = message.get_mut("content").and_then(|v| v.as_array_mut()) {
                    // Удаляем все thinking блоки
                    let before_len = content.len();
                    content.retain(|item| {
                        item.get("type").and_then(|t| t.as_str()) != Some("thinking")
                    });
                    result.stats.stripped_count += (before_len - content.len()) as u32;
                }
            }
        }

        result.changed = result.stats.stripped_count > 0;

        // Удаляем context_management если были изменения
        if result.changed {
            if let Some(obj) = body.as_object_mut() {
                obj.remove("context_management");
            }
        }

        Ok(result)
    }
}
```

#### SummarizeTransformer (будущее)

```rust
/// Режим summarize: нативная работа + суммаризация при switch.
pub struct SummarizeTransformer {
    summarizer_client: SummarizerClient,
    config: SummarizerConfig,
}

#[async_trait]
impl ThinkingTransformer for SummarizeTransformer {
    fn name(&self) -> &'static str { "summarize" }

    async fn transform_request(
        &self,
        _body: &mut Value,
        _context: &TransformContext,
    ) -> Result<TransformResult, TransformError> {
        // В обычном режиме - ничего не делаем (passthrough)
        Ok(TransformResult::default())
    }

    async fn on_backend_switch(
        &self,
        from: &str,
        to: &str,
        body: &mut Value,
    ) -> Result<(), TransformError> {
        // Суммаризируем все thinking блоки
        self.summarize_thinking_blocks(body).await
    }
}
```

### Registry / Factory

```rust
/// Реестр трансформеров с поддержкой горячей замены.
pub struct TransformerRegistry {
    current: Arc<RwLock<Box<dyn ThinkingTransformer>>>,
}

impl TransformerRegistry {
    pub fn new(config: &ThinkingConfig) -> Self {
        let transformer = Self::create_transformer(config);
        Self {
            current: Arc::new(RwLock::new(transformer)),
        }
    }

    fn create_transformer(config: &ThinkingConfig) -> Box<dyn ThinkingTransformer> {
        match config.mode {
            ThinkingMode::Strip => Box::new(StripTransformer),
            ThinkingMode::Summarize => Box::new(SummarizeTransformer::new(&config.summarizer)),
            ThinkingMode::Native => Box::new(NativeTransformer),
        }
    }

    /// Обновить конфигурацию (горячая замена)
    pub fn update_config(&self, config: &ThinkingConfig) {
        let transformer = Self::create_transformer(config);
        *self.current.write() = transformer;
    }

    pub fn get(&self) -> Arc<RwLock<Box<dyn ThinkingTransformer>>> {
        self.current.clone()
    }
}
```

## Интеграция с UpstreamClient

```rust
// upstream.rs

impl UpstreamClient {
    pub async fn do_forward(...) -> Result<Response<Body>, ProxyError> {
        // ...

        if request_content_type.contains("application/json") {
            let context = TransformContext {
                backend: backend.name.clone(),
                request_id: span.request_id().to_string(),
            };

            // Получаем текущий трансформер
            let transformer = self.transformer_registry.get();
            let transformer = transformer.read();

            // Трансформируем (async!)
            let result = transformer
                .transform_request(&mut json_body, &context)
                .await
                .map_err(|e| ProxyError::Internal(e.to_string()))?;

            if result.changed {
                body_bytes = serde_json::to_vec(&json_body)?;
                tracing::info!(
                    transformer = transformer.name(),
                    stats = ?result.stats,
                    "Transformed thinking blocks"
                );
            }
        }

        // ...
    }
}
```

## Миграция

### Шаг 1: Создать новую структуру файлов
```bash
src/proxy/thinking/
├── mod.rs
├── traits.rs
├── context.rs
└── strip.rs
```

### Шаг 2: Реализовать StripTransformer
- Перенести логику удаления из текущего `thinking.rs`
- Добавить тесты

### Шаг 3: Создать TransformerRegistry
- Интегрировать с конфигом
- Поддержка горячей замены

### Шаг 4: Обновить UpstreamClient
- Заменить `ThinkingTracker` на `TransformerRegistry`
- Сделать трансформацию async

### Шаг 5: Deprecate старый код
- `ThinkingMode::DropSignature` → `Strip`
- `ThinkingMode::ConvertToText` → удалить
- `ThinkingMode::ConvertToTags` → удалить (причина проблемы)

## Обратная совместимость

```toml
[thinking]
# Старые значения (deprecated, с предупреждением):
mode = "drop_signature"  # → strip
mode = "convert_to_text" # → strip + warning
mode = "convert_to_tags" # → strip + warning

# Новые значения:
mode = "strip"      # Удалять thinking блоки
mode = "summarize"  # Нативно + суммаризация при switch
mode = "native"     # Нативно + handoff при switch
```

## Тестирование

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn strip_transformer_removes_thinking_blocks() {
        let transformer = StripTransformer;
        let mut body = json!({
            "messages": [{
                "role": "assistant",
                "content": [
                    {"type": "thinking", "thinking": "my thoughts", "signature": "sig"},
                    {"type": "text", "text": "hello"}
                ]
            }]
        });

        let context = TransformContext {
            backend: "test".to_string(),
            request_id: "test-123".to_string(),
        };

        let result = transformer.transform_request(&mut body, &context).await.unwrap();

        assert!(result.changed);
        assert_eq!(result.stats.stripped_count, 1);

        // Проверяем что thinking блок удалён
        let content = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
    }
}
```

## Преимущества новой архитектуры

1. **Расширяемость**: Новый режим = новый файл с реализацией trait
2. **Тестируемость**: Каждый трансформер тестируется изолированно
3. **Async-ready**: `summarize` может делать HTTP запросы
4. **Горячая замена**: Можно менять режим без перезапуска
5. **Чистый код**: Разделение ответственности
6. **Типобезопасность**: Ошибки ловятся на этапе компиляции

## Зависимости

```toml
# Cargo.toml additions
async-trait = "0.1"      # Для async методов в trait objects
thiserror = "1.0"        # Для TransformError (уже используется)
```

## Альтернативы рассмотренные и отклонённые

### 1. Enum + match (текущий подход)
**Отклонён**: Вся логика в одном методе, не масштабируется, нет async.

### 2. Tower Layer
**Отклонён**: Overkill для нашего случая. Tower хорош для цепочек middleware, но мы выбираем ОДИН трансформер на основе конфига.

### 3. Dynamic plugin loading (FFI)
**Отклонён**: Требует отдельные бинарники, unstable ABI, избыточная сложность.

### 4. WASM plugins
**Отклонён**: Overhead, ограничения песочницы, сложность разработки.
