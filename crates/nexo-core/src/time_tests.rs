use crate::{CivilDate, CivilDateError, ValidityInterval, ValidityIntervalError};
#[test]
fn civil_date_rejects_invalid_calendar_days() {
    assert_eq!(
        CivilDate::try_new(2026, 2, 29),
        Err(CivilDateError::InvalidDay)
    );
    assert!(CivilDate::try_new(2024, 2, 29).is_ok());
    assert_eq!(
        CivilDate::try_new(0, 1, 1),
        Err(CivilDateError::UnsupportedYear)
    );
}

#[test]
fn validity_interval_is_closed_at_its_end_date() {
    let start = CivilDate::try_new(2026, 1, 1).unwrap();
    let end = CivilDate::try_new(2026, 2, 1).unwrap();
    let interval = ValidityInterval::try_new(start, Some(end)).unwrap();
    assert!(interval.contains(end));
    assert!(!interval.contains(CivilDate::try_new(2026, 2, 2).unwrap()));
}

#[test]
fn explicitly_open_ended_interval_contains_later_dates() {
    let start = CivilDate::try_new(2026, 1, 1).unwrap();
    let open_ended = ValidityInterval::try_new(start, None).unwrap();
    assert!(open_ended.contains(CivilDate::try_new(9999, 12, 31).unwrap()));
}
#[test]
fn validity_interval_rejects_reverse_time() {
    let start = CivilDate::try_new(2026, 1, 2).unwrap();
    let end = CivilDate::try_new(2026, 1, 1).unwrap();
    assert_eq!(
        ValidityInterval::try_new(start, Some(end)),
        Err(ValidityIntervalError::EndBeforeStart)
    );
}
