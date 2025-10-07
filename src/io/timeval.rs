use chrono::{DateTime, Local, NaiveDateTime, ParseError, TimeZone};

const TIMEVAL_FORMAT: &str = "%Y%m%d%H%M%S";

pub(crate) fn parse_timeval(input: &str) -> Result<Option<DateTime<Local>>, ParseError> {
  NaiveDateTime::parse_from_str(input, TIMEVAL_FORMAT)
    .map(|t| Local.from_local_datetime(&t).latest())
}

pub(crate) fn format_timeval(timeval: &DateTime<Local>) -> String {
  timeval.format(TIMEVAL_FORMAT).to_string()
}

#[cfg(test)]
mod tests {
  use chrono::NaiveDate;

  use super::*;

  #[test]
  fn valid_test() {
    let timeval = "20020717210715";
    let correct = Local
      .from_local_datetime(
        &NaiveDate::from_ymd_opt(2002, 7, 17).unwrap().and_hms_opt(21, 7, 15).unwrap(),
      )
      .unwrap();
    let parsed = parse_timeval(timeval);
    assert!(parsed.is_ok());
    assert!(parsed.unwrap().is_some());
    assert_eq!(correct, parsed.unwrap().unwrap());
  }

  #[test]
  fn empty_test() {
    let timeval = "";
    let parsed = parse_timeval(timeval);
    assert!(parsed.is_err())
  }

  #[test]
  fn leap_year_test() {
    let timeval = "20240214010203";
    let correct = Local
      .from_local_datetime(
        &NaiveDate::from_ymd_opt(2024, 2, 14).unwrap().and_hms_opt(1, 2, 3).unwrap(),
      )
      .unwrap();
    let parsed = parse_timeval(timeval);
    assert!(parsed.is_ok());
    assert!(parsed.unwrap().is_some());
    assert_eq!(correct, parsed.unwrap().unwrap());
  }
}
