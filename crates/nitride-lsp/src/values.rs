//! Statement values for inlay hints: the program runs with host handlers
//! chosen by the catalog's handler names, as the CLI's fixtures do.
use nitride_core::*;

/// Host handlers for every catalog operation, by its handler name.
pub fn bindings<'g>(language: &'g Language<'g>) -> Vec<HostBinding<'g>> {
    language
        .operations
        .iter()
        .zip(language.handlers)
        .map(|(signature, handler)| {
            if *handler == "iso-date" {
                return HostBinding::text(*signature, |text| {
                    iso_date_day_number(text)
                        .map(|day| day as f64)
                        .ok_or_else(|| "not a calendar date".into())
                });
            }
            let handler = handler.to_string();
            HostBinding::new(*signature, move |a| {
                let arg = |i: usize| {
                    a.get(i)
                        .copied()
                        .ok_or_else(|| "missing argument".to_owned())
                };
                Ok(match handler.as_str() {
                    "zero" => 0.0,
                    "identity" => arg(0)?,
                    "neg" => -arg(0)?,
                    "add" => arg(0)? + arg(1)?,
                    "sub" => arg(0)? - arg(1)?,
                    "mul" => arg(0)? * arg(1)?,
                    "div" => arg(0)? / arg(1)?,
                    "pow" => arg(0)?.powf(arg(1)?),
                    _ => return Err("unknown host handler".into()),
                })
            })
        })
        .collect()
}

/// Today as a day number (days since 0001-01-01), in UTC.
fn today() -> f64 {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    (719_162 + seconds / 86_400) as f64
}

/// Host values for the built-in values the language declares that have a
/// known meaning; others get none, so the program will not run.
pub fn host_values(language: &Language<'_>) -> Vec<HostValue> {
    let mut values: Vec<HostValue> = Vec::new();
    let declared = language.rules.iter().flat_map(|r| match r.action {
        Action::Reference { values, .. } => values,
        _ => &[],
    });
    for value in declared {
        let known = match (value.kind, value.name, value.ty) {
            ("value", "pi", "Core.Scalar") => {
                HostValue::new("value", "pi", "Core.Scalar", || Ok(std::f64::consts::PI))
            }
            ("value", "e", "Core.Scalar") => {
                HostValue::new("value", "e", "Core.Scalar", || Ok(std::f64::consts::E))
            }
            ("value", "tau", "Core.Scalar") => {
                HostValue::new("value", "tau", "Core.Scalar", || Ok(std::f64::consts::TAU))
            }
            ("value", "today", "Dates.Date") => {
                HostValue::new("value", "today", "Dates.Date", || Ok(today()))
            }
            _ => continue,
        };
        if !values.iter().any(|v| v.name == known.name) {
            values.push(known);
        }
    }
    values
}

/// `yyyy-MM-dd` of a day number, the inverse of `iso_date_day_number`.
pub fn iso_date(day: i64) -> String {
    // Howard Hinnant's civil_from_days, shifted from 1970-01-01 to 0001-01-01.
    let z = day - 719_162 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

fn label(value: f64, ty: &str) -> String {
    let number = || {
        if value.fract() == 0.0 && value.abs() < 1e15 {
            format!("{}", value as i64)
        } else {
            format!("{value}")
        }
    };
    if ty == "Core.Scalar" {
        format!("= {}", number())
    } else if ty.ends_with("Date") && value.fract() == 0.0 && value.abs() < 1e9 {
        format!("= {}", iso_date(value as i64))
    } else {
        format!("= {} {}", number(), ty.rsplit('.').next().unwrap_or(ty))
    }
}

/// `(end offset, label)` for each show statement, or none when the program
/// does not run to completion.
pub fn compute(program: &Program<'_>) -> Vec<(u32, String)> {
    let language = program.language;
    let bindings = bindings(language);
    let host = host_values(language);
    let Ok(report) = execute_with_values(program, &bindings, &host, Limits::default()) else {
        return vec![];
    };
    program
        .statements
        .iter()
        .filter(|s| s.symbol.is_none())
        .zip(report.values)
        .filter_map(|(statement, value)| {
            let node = program.nodes.get(statement.value as usize)?;
            Some((node.span[1], label(value, node.ty)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_round_trip() {
        let last = iso_date_day_number("9999-12-31").unwrap();
        let mut day = 0;
        while day <= last {
            assert_eq!(iso_date_day_number(&iso_date(day)), Some(day), "{day}");
            day += 97;
        }
        assert_eq!(iso_date(0), "0001-01-01");
    }
}
