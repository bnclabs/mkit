pub fn to_snake_case(s: &str) -> String {
    use std::iter::FromIterator;

    let mut snake_case = vec![];
    for ch in s.chars() {
        if ch.is_uppercase() {
            snake_case.push('_');
        }
        ch.to_lowercase().for_each(|ch| snake_case.push(ch));
    }
    String::from_iter(snake_case.into_iter())
}
