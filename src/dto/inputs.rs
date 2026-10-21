//! Validated wire values. No sanitization: accepted input is preserved verbatim.

use nutype::nutype;

#[nutype(
    validate(len_char_max = 200, predicate = nonblank_without_nul),
    derive(Deserialize, AsRef)
)]
pub struct DisplayName(String);

#[nutype(
    validate(len_char_max = 200, predicate = nonblank_without_nul),
    derive(Deserialize, AsRef)
)]
pub struct LessonId(String);

#[nutype(
    validate(len_char_max = 200, predicate = nonblank_without_nul),
    derive(Deserialize, AsRef)
)]
pub struct ExerciseId(String);

#[nutype(
    validate(predicate = |value: &str| value == "exercise_completed"),
    derive(Deserialize, AsRef)
)]
pub struct ExerciseEventType(String);

#[nutype(validate(greater_or_equal = 0), derive(Clone, Copy, Deserialize))]
pub struct XpAwarded(i32);

#[nutype(
    validate(greater_or_equal = 1, less_or_equal = 100),
    default = 20,
    derive(Clone, Copy, Default, Deserialize)
)]
pub struct PageLimit(u32);

#[nutype(
    validate(less_or_equal = 10_000),
    default = 0,
    derive(Clone, Copy, Default, Deserialize)
)]
pub struct PageOffset(u32);

#[nutype(
    validate(len_char_max = 100, predicate = nonblank_without_nul),
    derive(Deserialize, AsRef)
)]
pub struct EventTypeFilter(String);

// Header validation borrows its input: no extra String allocation for the key.
#[nutype(validate(predicate = valid_idempotency_key))]
pub struct IdempotencyKey<'a>(&'a str);

fn nonblank_without_nul(value: &str) -> bool {
    !value.trim().is_empty() && !value.contains('\0')
}

fn valid_idempotency_key(value: &&str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 200
        && value.bytes().all(|byte| (b' '..=b'~').contains(&byte))
}
