# live-core — bootstrap reference

**Статус:** исторический bootstrap-документ. Canonical implementation repo:
`xodapi/live-core`.

Этот файл не заменяет полную спецификацию. Он фиксирует минимальный
проверяемый контракт для старта, потому что в текущем `main` отсутствуют
ранее упомянутые reference-файлы:

- `LIVE_CORE_FULL_SPEC.ru.md`
- `LIVE_CORE_SECURITY_AUDIT.ru.md`

На момент исходной фиксации ещё не существовало отдельного
GitHub-репозитория `xodapi/live-core`. Поэтому первый шаг live-core должен был
быть маленьким и проверяемым, а не новым раундом исследования.

---

## 1. Bootstrap scope

Разрешено стартовать только первые три модуля, уже зафиксированные в планах:

1. `clock` — `Clock` + `Instant`
2. `rolling_buffer` — фиксированный кольцевой буфер v1
3. `threshold` — чистая оценка уровней порогов

Не начинать сейчас:

- Rig / rig-core
- jj / git-cliff
- Bevy / 3D
- iOS
- poller / async runtime
- Observer trait
- state_machine
- offline_tracker
- percentile/median
- ExponentialSmoother
- serde-сериализация буферов

`ExponentialSmoother` остаётся v2-идеей из #163: полезно записать в будущую
полную спецификацию, но не включать в bootstrap v1.

---

## 2. Repository bootstrap

Операционный шаг перед кодом:

1. Создать `xodapi/live-core`.
2. Инициализировать Rust library crate.
3. Добавить `AGENTS.md` и `MULTI-AGENT.md`, адаптированные под live-core.
4. Скопировать этот bootstrap-документ как reference.
5. Позже восстановить или заново собрать полные:
   - `LIVE_CORE_FULL_SPEC.ru.md`
   - `LIVE_CORE_SECURITY_AUDIT.ru.md`

После появления отдельного репозитория live-core задачи нужно держать в
`xodapi/live-core`. Код live-core не должен смешиваться с кодом vimit.

---

## 3. Common requirements

Для каждого live-core module issue:

- `cargo test --locked`
- `cargo clippy --all-targets -- -D warnings`
- `cargo fmt --check`
- публичные типы без доменных слов `client`, `http`, `connection`
- без сетевого I/O
- без API keys, prompts, paths, transcripts, task titles
- без runtime-зависимости от vimit

Желательная baseline-цель: library crate с `std` feature по умолчанию и
подготовкой к `no_std`, если это не раздувает первый PR.

---

## 4. Module 1: clock

Цель: отделить понятие времени от `std::time::Instant`, чтобы остальные
модули могли тестироваться детерминированно и позже жить в embedded/no_std
контекстах.

Минимальный контракт:

```rust
pub trait Clock {
    fn now(&self) -> Instant;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Instant(...);

impl Instant {
    pub fn duration_since(self, earlier: Instant) -> core::time::Duration;
}
```

`StdClock` разрешён только под `std` feature:

```rust
#[cfg(feature = "std")]
pub struct StdClock;
```

Acceptance:

- `Clock` trait покрыт тестовым clock/fake clock.
- `Instant` является newtype, а не публичным re-export `std::time::Instant`.
- `duration_since` имеет тесты на равные timestamps и обычный положительный
  интервал.
- no_std-путь не требует `StdClock`.

---

## 5. Module 2: rolling_buffer

Цель: маленький фиксированный буфер последних значений для live-метрик.

Минимальный контракт:

```rust
pub struct RollingBuffer<T, const N: usize> { ... }
```

Для числового v1 достаточно поддержать:

- `push(value)`
- `len()`
- `is_empty()`
- `capacity()`
- итерацию в порядке от старого к новому
- `mean()` для `f64` или отдельного numeric wrapper, если это проще и чище

Acceptance:

- `N = 0` либо явно запрещён типом/API, либо ведёт себя документированно.
- Пустой буфер не panics.
- Один элемент возвращает mean этого элемента.
- Переполнение вытесняет самые старые значения.
- Tests покрывают wrap-around.

Не включать в v1:

- percentile
- median
- ExponentialSmoother
- serde
- heap allocation as a requirement

---

## 6. Module 3: threshold

Цель: чистая функция/тип для классификации метрик в `ok`, `warning`,
`danger`, без UI и без API.

Минимальный контракт:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Level {
    Ok,
    Warning,
    Danger,
}

pub struct Threshold {
    pub warning: f64,
    pub danger: f64,
}

impl Threshold {
    pub fn classify(&self, value: f64) -> Level;
}
```

Acceptance:

- `warning < danger` валидируется.
- Boundary tests: ниже warning, ровно warning, между warning/danger, ровно
  danger, выше danger.
- NaN/inf handling явно определён и покрыт тестами.
- Нет зависимости от UI/CLI/vimit parse structs.

---

## 7. What to create next

После merge этого документа создать отдельные issues:

- `chore(live-core): create repository scaffold`
- `feat(live-core/clock): add Clock and Instant`
- `feat(live-core/buffer): add RollingBuffer v1`
- `feat(live-core/threshold): add Threshold classifier`

`buffer` и `threshold` можно делать параллельно после scaffold. `clock`
лучше первым, потому что дальше от него зависят `offline_tracker`,
`state_machine` и `poller`.
