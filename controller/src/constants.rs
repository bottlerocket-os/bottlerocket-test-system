use crate::error::Result;
use anyhow::Context;
use kube_runtime::controller::Action;
use std::collections::VecDeque;
use std::time::Duration;

const UNITS: [(char, u64); 3] = [('d', 86400), ('h', 3600), ('m', 60)];

/// Tell the controller to reconcile the object again after some duration.
pub(crate) fn requeue() -> Action {
    Action::requeue(Duration::from_secs(5))
}

/// Requeue just in case, but we don't expect anything to happen.
pub(crate) fn requeue_slow() -> Action {
    Action::requeue(Duration::from_secs(30))
}

/// Do not requeue the object.
pub(crate) fn no_requeue() -> Action {
    Action::await_change()
}

/// Parse a Duration string into a Duration object.
pub(crate) fn parse_duration(input: &str) -> Result<Duration> {
    let mut secs: u64 = 0;
    let mut duration_string = input;
    for unit in UNITS {
        let mut vec: VecDeque<&str> = duration_string.split(unit.0).collect();
        if vec.len() > 1 {
            secs += vec
                .pop_front()
                .context("Failed to parse input")?
                .parse::<u64>()?
                * unit.1;
        }
        duration_string = vec.pop_front().context("Failed to parse input")?;
    }
    let mut vec: VecDeque<&str> = duration_string.split('s').collect();
    let seconds = vec.pop_front().context("Failed to parse input")?;
    if !seconds.is_empty() {
        secs += seconds.parse::<u64>()?;
    }
    Ok(Duration::from_secs(secs))
}

#[test]
fn all_units() {
    let input = "1d2h3m4s";
    assert!(
        parse_duration(input).is_ok()
            && parse_duration(input).unwrap() == Duration::from_secs(93784)
    )
}

#[test]
fn some_units() {
    let input = "1d3m4s";
    assert!(
        parse_duration(input).is_ok()
            && parse_duration(input).unwrap() == Duration::from_secs(86584)
    )
}

#[test]
fn only_seconds() {
    let input = "500s";
    assert!(
        parse_duration(input).is_ok() && parse_duration(input).unwrap() == Duration::from_secs(500)
    )
}

#[test]
fn no_seconds() {
    let input = "1h5m";
    assert!(
        parse_duration(input).is_ok()
            && parse_duration(input).unwrap() == Duration::from_secs(3900)
    )
}

#[test]
fn one_unit() {
    let input = "10m";
    assert!(
        parse_duration(input).is_ok() && parse_duration(input).unwrap() == Duration::from_secs(600)
    )
}

#[test]
fn no_units() {
    let input = "5123";
    assert!(
        parse_duration(input).is_ok()
            && parse_duration(input).unwrap() == Duration::from_secs(5123)
    )
}

#[test]
fn wrong_order() {
    let input = "10d5m3h2s";
    assert!(parse_duration(input).is_err())
}

#[test]
fn invalid_unit() {
    let input = "5y40s";
    assert!(parse_duration(input).is_err())
}

#[test]
fn missing_value() {
    let input = "5hm4s";
    assert!(parse_duration(input).is_err())
}
