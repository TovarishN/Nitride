//! Statement values for inlay hints: the program runs with the built-in host
//! handlers the catalog names, as the CLI's fixtures do.
use nitride_core::*;

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
    let bindings = host::bindings(language);
    let host = host::host_values(language, host::utc_today());
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
