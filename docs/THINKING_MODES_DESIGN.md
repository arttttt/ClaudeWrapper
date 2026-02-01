# Thinking Modes Design

## Проблема

При использовании ClaudeWrapper в режиме `convert_to_tags` thinking блоки конвертируются в обычный текст с тегами `<think>...</think>`. Это создаёт проблемы:

1. **Накопление контекста**: API Anthropic не распознаёт `<think>` как thinking блоки и не применяет автоматическое stripping/суммаризацию
2. **Потеря суммаризации**: Anthropic использует отдельную модель для суммаризации thinking блоков — при конвертации в текст эта функция теряется
3. **Раздувание контекста**: Сырые thinking блоки накапливаются, занимая место в контексте
4. **Зависание extended thinking**: В некоторых случаях это приводит к зависанию стрима

## Решение: Три режима работы с thinking

### Обзор режимов

| Режим | Backend switching | Thinking контекст | Перезапуск Claude | Сложность |
|-------|------------------|-------------------|-------------------|-----------|
| `strip` | Мгновенно | Теряется | Нет | Низкая |
| `summarize` | С суммаризацией | Сохраняется (сжато) | **Нет** | Средняя |
| `native` | Через handoff | Полный (нативный) | **Да** | Высокая |

### Когда какой режим использовать

- **`strip`** — когда thinking не важен, или нужна максимальная стабильность
- **`summarize`** — **рекомендуется** для большинства случаев; баланс между сохранением контекста и гибкостью
- **`native`** — для сложных задач, где важен полный thinking контекст и редко нужно переключать backend

---

## Режим 1: `strip`

### Описание

Полное удаление thinking блоков из запросов. Самый простой режим, который гарантирует стабильную работу при переключении backends.

### Поведение

1. При получении запроса с thinking блоками в истории
2. Thinking блоки **полностью удаляются** (не конвертируются в текст)
3. Запрос отправляется без thinking контекста
4. Переключение backends работает без ограничений

### Конфигурация

```toml
[thinking]
mode = "strip"
```

### Преимущества

- Простая реализация
- Нет накопления контекста
- Стабильная работа с любыми backends
- Нет зависимостей

### Недостатки

- Потеря контекста размышлений между ходами
- Модель "забывает" свои предыдущие рассуждения

### Реализация

```rust
// В thinking.rs
ThinkingMode::Strip => {
    // Полностью удаляем thinking блок из массива content
    items_to_remove.push(index);
    result.strip_count = result.strip_count.saturating_add(1);
    changed = true;
}
```

### Открытые вопросы

- [ ] Нужно ли логировать количество удалённых блоков?
- [ ] Показывать ли предупреждение пользователю при первом удалении?

---

## Режим 2: `summarize`

### Описание

Гибридный режим: во время работы с одним backend thinking блоки **сохраняются в нативном формате** (как в режиме `native`). Суммаризация происходит **только в момент переключения backend** — все накопленные thinking блоки суммаризируются через отдельную модель и заменяются на текстовое резюме.

### Ключевое отличие от других режимов

| Аспект | `strip` | `summarize` | `native` |
|--------|---------|-------------|----------|
| Во время работы | Удаление | **Нативный формат** | Нативный формат |
| При переключении | Просто удалить | **Суммаризировать** | Handoff + restart |
| Перезапуск Claude | Нет | **Нет** | Да |

### Поведение

**Обычная работа (без переключения backend):**
1. Thinking блоки передаются **как есть** (нативный формат Anthropic)
2. API Anthropic сам управляет thinking (автосуммаризация, stripping)
3. Полноценная работа extended thinking
4. Нет дополнительных API вызовов

**При переключении backend:**
1. Пользователь запрашивает смену backend (например, Anthropic → GLM)
2. ClaudeWrapper показывает UI: "Preparing switch..."
3. Все thinking блоки в истории суммаризируются через summarizer модель
4. Thinking блоки заменяются на текстовые блоки с резюме
5. Переключение завершается
6. **Claude продолжает работу без перезапуска** — новые запросы идут к новому backend

### Конфигурация

```toml
[thinking]
mode = "summarize"

[thinking.summarizer]
# Какой backend использовать для суммаризации
backend = "glm"  # Имя из секции [[backends]]

# Опционально: конкретная модель (если backend поддерживает несколько)
model = "glm-4-flash"

# Максимальное количество токенов в резюме
max_tokens = 500

# Формат вывода резюме
output_format = "text"  # "text" | "xml" | "json"

# Кэширование резюме (по хэшу содержимого)
cache_enabled = true
cache_ttl_seconds = 3600
```

### Промпт для суммаризации

```
[SYSTEM]
You are a summarization assistant. Summarize the following AI reasoning/thinking content.
Focus on:
- Key decisions made
- Important findings or discoveries
- Current plan or next steps
- Critical context that should be preserved

Be concise but preserve essential information. Output only the summary, no preamble.

[USER]
{thinking_content}
```

### Формат вывода

**Вариант 1: Plain text**
```
Summary: Analyzed codebase structure, found race condition in port binding.
Decision: Use atomic port allocation. Next: implement fix in proxy/server.rs
```

**Вариант 2: XML tags**
```xml
<thinking-summary>
Analyzed codebase structure, found race condition in port binding.
Decision: Use atomic port allocation. Next: implement fix in proxy/server.rs
</thinking-summary>
```

**Вариант 3: JSON**
```json
{"type": "thinking_summary", "content": "..."}
```

### Flow переключения backend

```
┌─────────────────────────────────────────────────────────────────┐
│                  ClaudeWrapper (summarize mode)                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  [Обычная работа: thinking нативный, API сам его обрабатывает]  │
│                                                                  │
│  User: "Switch to GLM"                                          │
│           │                                                      │
│           ▼                                                      │
│  ┌─────────────────────────────────────────┐                    │
│  │ UI: "Preparing backend switch..."       │                    │
│  │      [████████░░░░░░] Summarizing...    │                    │
│  └─────────────────────────────────────────┘                    │
│           │                                                      │
│           ▼                                                      │
│  ┌─────────────────────────────────────────┐                    │
│  │ Для каждого thinking блока в истории:   │                    │
│  │  → Отправить к summarizer (GLM-flash)   │                    │
│  │  → Получить сжатое резюме               │                    │
│  │  → Заменить thinking → text             │                    │
│  └─────────────────────────────────────────┘                    │
│           │                                                      │
│           ▼                                                      │
│  ┌─────────────────────────────────────────┐                    │
│  │ История теперь содержит:                │                    │
│  │  - Обычные сообщения (без изменений)    │                    │
│  │  - Резюме вместо thinking блоков        │                    │
│  └─────────────────────────────────────────┘                    │
│           │                                                      │
│           ▼                                                      │
│  ┌─────────────────────────────────────────┐                    │
│  │ Переключить active backend → GLM        │                    │
│  │ Claude продолжает без перезапуска       │                    │
│  └─────────────────────────────────────────┘                    │
│           │                                                      │
│           ▼                                                      │
│  Пользователь продолжает работу с GLM backend                   │
│  (thinking резюме сохранены в истории как text)                 │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Преимущества

- **Нативная работа thinking** во время обычной сессии
- API сам оптимизирует thinking (суммаризация, кэширование)
- Thinking блоки не считаются в контекст (API их strip-ит)
- Сохраняет контекст размышлений при переключении
- **Не требует перезапуска Claude** (в отличие от `native`)
- Суммаризация происходит **только при переключении**, не на каждый запрос

### Недостатки

- Добавляет latency **при переключении** (запрос к summarizer)
- Требует настройки summarizer backend
- Потеря деталей при суммаризации (только при switch)
- После переключения новый backend не видит "нативный" thinking

### Реализация

```rust
// Summarizer вызывается только при переключении backend
pub struct ThinkingSummarizer {
    client: Client,
    config: SummarizerConfig,
    cache: Option<LruCache<String, String>>,
}

impl ThinkingSummarizer {
    /// Суммаризировать все thinking блоки в истории
    /// Вызывается ТОЛЬКО при переключении backend
    pub async fn summarize_history(
        &self,
        messages: &mut Vec<Message>
    ) -> Result<SummarizeResult> {
        let mut summarized_count = 0;

        for message in messages.iter_mut() {
            for content in message.content.iter_mut() {
                if content.is_thinking_block() {
                    let thinking = content.get_thinking_text()?;

                    // Проверить кэш
                    let summary = if let Some(cached) = self.check_cache(&thinking) {
                        cached
                    } else {
                        let result = self.call_summarizer(&thinking).await?;
                        self.cache_summary(&thinking, &result);
                        result
                    };

                    // Заменить thinking блок на текстовый
                    *content = Content::Text {
                        text: format!("<thinking-summary>{}</thinking-summary>", summary),
                    };
                    summarized_count += 1;
                }
            }
        }

        Ok(SummarizeResult { summarized_count })
    }
}

// В логике переключения backend
pub async fn switch_backend(&mut self, new_backend: &str) -> Result<()> {
    if self.thinking_mode == ThinkingMode::Summarize {
        // Показать UI прогресса
        self.show_progress("Preparing backend switch...");

        // Суммаризировать все thinking блоки
        let result = self.summarizer.summarize_history(&mut self.message_history).await?;

        tracing::info!(
            summarized = result.summarized_count,
            "Summarized thinking blocks for backend switch"
        );
    }

    // Переключить backend (без перезапуска Claude)
    self.active_backend = new_backend.to_string();
    Ok(())
}
```

### Открытые вопросы

- [ ] Как обрабатывать ошибки summarizer? Fallback на strip?
- [ ] Суммаризировать каждый thinking блок отдельно или batch?
- [ ] Нужен ли rate limiting для summarizer запросов?
- [ ] Как показывать прогресс суммаризации в TUI?
- [ ] Кэшировать ли резюме на диск (для повторных switch)?
- [ ] Что делать при частом переключении туда-обратно?

---

## Режим 3: `native`

### Описание

Thinking блоки сохраняются в нативном формате API Anthropic. Это позволяет API самостоятельно управлять thinking (суммаризация, stripping). При переключении backend **требуется полный перезапуск Claude** с handoff.

### Отличие от `summarize`

| Аспект | `summarize` | `native` |
|--------|-------------|----------|
| Суммаризация | Внешняя модель (при switch) | Сам Claude (handoff) |
| Перезапуск | **Нет** | **Да** |
| История | Сохраняется | Только summary |
| Сложность | Средняя | Высокая |

> **Когда выбрать `native` вместо `summarize`:** Когда критически важно, чтобы handoff summary делал сам Claude (лучше понимает контекст), и допустима потеря истории сообщений.

### Поведение

**Обычная работа:**
1. Thinking блоки передаются как есть (без трансформации)
2. API Anthropic сам управляет thinking (суммаризация меньшей моделью)
3. Переключение backends **заблокировано**

**При попытке переключения backend:**
1. ClaudeWrapper показывает предупреждение
2. Пользователь подтверждает переключение
3. Текущая сессия Claude получает запрос на создание handoff summary
4. Summary сохраняется в память ClaudeWrapper
5. Текущий процесс Claude завершается
6. Новый процесс Claude запускается с новым backend
7. Handoff summary инжектится в новую сессию

### Конфигурация

```toml
[thinking]
mode = "native"

[thinking.native]
# Показывать предупреждение при попытке переключения
warn_on_switch = true

# Автоматически создавать handoff (без подтверждения)
auto_handoff = false

# Промпт для создания handoff summary
handoff_prompt = """
Please create a concise summary of our conversation for handoff to a new session.
Include:
- What we were working on
- Key decisions made
- Current state of the work
- What remains to be done
"""

# Максимальная длина handoff summary
max_handoff_tokens = 2000

# Формат инжекта handoff в новую сессию
handoff_inject_method = "first_message"  # "first_message" | "system_prompt" | "claude_md"
```

### Flow переключения backend

```
┌─────────────────────────────────────────────────────────────────┐
│                    ClaudeWrapper (native mode)                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  User: "Switch to GLM"                                          │
│           │                                                      │
│           ▼                                                      │
│  ┌─────────────────────────────────────────┐                    │
│  │ Warning: Native mode requires handoff.  │                    │
│  │ Current thinking context will be        │                    │
│  │ summarized. Continue? [Y/n]             │                    │
│  └─────────────────────────────────────────┘                    │
│           │                                                      │
│           ▼ (User confirms)                                      │
│  ┌─────────────────────────────────────────┐                    │
│  │ Request handoff summary from            │                    │
│  │ current Claude session (anthropic)      │                    │
│  └─────────────────────────────────────────┘                    │
│           │                                                      │
│           ▼                                                      │
│  ┌─────────────────────────────────────────┐                    │
│  │ Claude (anthropic) returns summary:     │                    │
│  │ "We were implementing thinking modes..."│                    │
│  └─────────────────────────────────────────┘                    │
│           │                                                      │
│           ▼                                                      │
│  ┌─────────────────────────────────────────┐                    │
│  │ Store summary in memory                 │                    │
│  │ Kill Claude process (anthropic)         │                    │
│  │ Start Claude process (glm)              │                    │
│  └─────────────────────────────────────────┘                    │
│           │                                                      │
│           ▼                                                      │
│  ┌─────────────────────────────────────────┐                    │
│  │ Inject handoff into new session:        │                    │
│  │ "[HANDOFF] {summary} [/HANDOFF]"        │                    │
│  └─────────────────────────────────────────┘                    │
│           │                                                      │
│           ▼                                                      │
│  New session continues with GLM backend                         │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Преимущества

- Полноценная работа extended thinking
- API сам оптимизирует thinking (суммаризация, кэширование)
- Thinking блоки не считаются в контекст (API их strip-ит)
- Лучшее качество рассуждений

### Недостатки

- Нельзя переключать backends без перезапуска
- Сложная реализация handoff
- Потеря истории сообщений при handoff (только summary)
- Требуется UI для предупреждений и подтверждений

### Реализация

```rust
// В backend switching logic
pub async fn switch_backend(&mut self, new_backend: &str) -> Result<()> {
    if self.thinking_mode == ThinkingMode::Native {
        // 1. Показать предупреждение и получить подтверждение
        if !self.confirm_native_switch().await? {
            return Ok(()); // User cancelled
        }

        // 2. Запросить handoff summary
        let summary = self.request_handoff_summary().await?;

        // 3. Сохранить summary
        self.pending_handoff = Some(HandoffData {
            summary,
            source_backend: self.active_backend.clone(),
            target_backend: new_backend.to_string(),
            timestamp: Utc::now(),
        });

        // 4. Завершить текущий процесс
        self.pty.terminate().await?;

        // 5. Сменить backend
        self.active_backend = new_backend.to_string();

        // 6. Запустить новый процесс
        self.spawn_claude_process().await?;

        // 7. Инжектить handoff
        self.inject_handoff().await?;

        Ok(())
    } else {
        // Для других режимов — обычное переключение
        self.active_backend = new_backend.to_string();
        Ok(())
    }
}
```

### Открытые вопросы

- [ ] Как показывать предупреждение в TUI? Popup? Отдельный prompt?
- [ ] Как обрабатывать ситуацию, когда Claude не может создать summary?
- [ ] Сохранять ли handoff на диск (для recovery)?
- [ ] Как инжектить handoff? stdin, system prompt, CLAUDE.md?
- [ ] Нужно ли сохранять историю handoff-ов?
- [ ] Как обрабатывать handoff при аварийном завершении?

---

## Конфигурация

### Полная схема конфига

```toml
[thinking]
# Режим работы с thinking блоками
# - "strip": удалять thinking блоки (default)
# - "summarize": суммаризировать через отдельную модель
# - "native": сохранять нативный формат, handoff при смене backend
mode = "strip"

# ============================================
# Настройки для режима "summarize"
# ============================================
[thinking.summarizer]
# Backend для суммаризации (из [[backends]])
backend = "glm"

# Конкретная модель (опционально, если backend поддерживает)
model = "glm-4-flash"

# Максимум токенов в резюме
max_tokens = 500

# Формат вывода: "text", "xml", "json"
output_format = "text"

# Кастомный промпт (опционально)
# prompt = "Summarize the following thinking..."

# Кэширование
cache_enabled = true
cache_ttl_seconds = 3600

# Fallback при ошибке summarizer
fallback_mode = "strip"  # "strip" | "error"

# ============================================
# Настройки для режима "native"
# ============================================
[thinking.native]
# Предупреждение при переключении backend
warn_on_switch = true

# Автоматический handoff без подтверждения
auto_handoff = false

# Промпт для handoff summary
handoff_prompt = """
Create a concise summary for session handoff:
- What we were working on
- Key decisions made
- Current state
- Remaining tasks
"""

# Максимум токенов в handoff
max_handoff_tokens = 2000

# Метод инжекта: "first_message", "system_prompt", "claude_md"
handoff_inject_method = "first_message"

# Сохранять handoff на диск
persist_handoff = false
handoff_path = "~/.cache/claudewrapper/handoffs/"
```

### Миграция с текущего конфига

Текущий конфиг:
```toml
[thinking]
mode = "convert_to_tags"
```

Новый конфиг (обратная совместимость):
```toml
[thinking]
# "convert_to_tags" → deprecated, трактуется как "strip" с предупреждением
mode = "strip"
```

---

## План реализации

### Фаза 1: `strip` режим
**Приоритет: Высокий**
**Сложность: Низкая**

- [ ] Добавить `Strip` вариант в `ThinkingMode` enum
- [ ] Реализовать удаление thinking блоков в `transform_request`
- [ ] Обновить конфиг парсер
- [ ] Добавить тесты
- [ ] Обновить документацию
- [ ] Deprecate `convert_to_tags` режим

### Фаза 2: `summarize` режим (рекомендуемый)
**Приоритет: Высокий**
**Сложность: Средняя**

Ключевое отличие от первоначального дизайна: суммаризация происходит **только при переключении backend**, а не на каждый запрос.

- [ ] Реализовать passthrough thinking блоков при обычной работе
- [ ] Спроектировать интерфейс `ThinkingSummarizer`
- [ ] Реализовать HTTP клиент для summarizer
- [ ] Добавить конфигурацию `[thinking.summarizer]`
- [ ] Интегрировать в логику `switch_backend`
- [ ] Добавить UI прогресса "Preparing switch..."
- [ ] Реализовать кэширование (опционально)
- [ ] Добавить fallback на `strip` при ошибках
- [ ] Добавить тесты
- [ ] Написать документацию

### Фаза 3: `native` режим
**Приоритет: Низкий**
**Сложность: Высокая**

Ключевое отличие от `summarize`: требует **перезапуска Claude** при переключении backend. Это более сложный сценарий, но сохраняет полный thinking контекст в нативном формате.

- [ ] Реализовать блокировку переключения backend
- [ ] Добавить UI для предупреждений (TUI popup)
- [ ] Реализовать запрос handoff summary от текущей сессии Claude
- [ ] Реализовать сохранение handoff в память
- [ ] Реализовать **перезапуск Claude процесса**
- [ ] Реализовать инжект handoff в новую сессию
- [ ] Добавить конфигурацию `[thinking.native]`
- [ ] Добавить persistence (опционально)
- [ ] Добавить тесты
- [ ] Написать документацию

> **Примечание:** Рассмотреть реализацию только после того, как `summarize` режим покажет недостаточное качество сохранения контекста.

---

## Appendix

### A. Как Anthropic обрабатывает thinking

Из документации Anthropic:

> "Thinking blocks from previous turns are stripped and not counted towards your context window"

> "With extended thinking enabled, the Messages API for Claude 4 models returns a summary of Claude's full thinking process. Summarization is processed by a different model than the one you target in your requests."

> "Summarization preserves the key ideas of Claude's thinking process with minimal added latency"

### B. Текущая реализация `convert_to_tags`

```rust
ThinkingMode::ConvertToTags => {
    *item = serde_json::json!({
        "type": "text",
        "text": format!("<think>{}</think>", text),
    });
    result.tag_count = result.tag_count.saturating_add(1);
    changed = true;
}
```

Проблема: API видит это как обычный текст и не применяет thinking-специфичную логику.

### C. Ссылки

- [Anthropic Extended Thinking Docs](https://platform.claude.com/docs/en/build-with-claude/extended-thinking)
- [Context Editing Docs](https://platform.claude.com/docs/en/build-with-claude/context-editing)
- [LiteLLM Reasoning Content](https://docs.litellm.ai/docs/reasoning_content)
- [OpenRouter Reasoning Tokens](https://openrouter.ai/docs/guides/best-practices/reasoning-tokens)
