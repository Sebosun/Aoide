//! Comma-separated values as exports write them (RFC 4180): fields in double quotes may hold
//! commas, line breaks and doubled quotes. Enough for an export; not a general CSV library.

/// Every record in `text`, each a list of fields.
pub fn records(text: &str) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quoted, c) {
            (true, '"') if chars.peek() == Some(&'"') => {
                chars.next();
                field.push('"');
            }
            (true, '"') => quoted = false,
            (true, c) => field.push(c),
            (false, '"') if field.is_empty() => quoted = true,
            (false, ',') => record.push(std::mem::take(&mut field)),
            (false, '\r') => {}
            (false, '\n') => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            (false, c) => field.push(c),
        }
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records
}

#[cfg(test)]
mod tests {
    use super::records;

    #[test]
    fn quoted_fields_hold_commas_quotes_and_line_breaks() {
        let text = "a,b,c\r\n\"one, two\",\"say \"\"hi\"\"\",\"line\nbreak\"\nlast,,\n";
        assert_eq!(
            records(text),
            [
                vec!["a", "b", "c"],
                vec!["one, two", "say \"hi\"", "line\nbreak"],
                vec!["last", "", ""],
            ]
        );
        assert_eq!(records("x,y"), [vec!["x", "y"]]);
    }
}
