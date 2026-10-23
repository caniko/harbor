fn greet(name: &str) -> String {
    format!("hello, {name}")
}

fn main() {
    println!("{}", greet("harbor"));
}

#[cfg(test)]
mod tests {
    #[test]
    fn greet() {
        assert_eq!(super::greet("x"), "hello, x");
    }
}
