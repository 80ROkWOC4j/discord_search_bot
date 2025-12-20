use crate::command::search::logic::substr;

#[test]
fn test_substr() {
    assert_eq!(substr("hello world", 20), "hello world");
    assert_eq!(substr("hello world", 5), "hello");
    // utf-8 korean(3bytes)
    assert_eq!(substr("안녕하세요", 7), "안녕하세요");
    assert_eq!(substr("안녕하세요", 2), "안녕");
}
