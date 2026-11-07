#[test]
fn test_rusqlite_in_memory_connection() {
    let conn = rusqlite::Connection::open_in_memory().expect("in-memory db must open");
    conn.execute(
        "CREATE TABLE test (id INTEGER PRIMARY KEY, name TEXT)",
        [],
    ).expect("create table must succeed");
    conn.execute("INSERT INTO test (name) VALUES (?1)", ["hq"]).expect("insert must succeed");
    let name: String = conn.query_row("SELECT name FROM test WHERE id = 1", [], |row| row.get(0)).unwrap();
    assert_eq!(name, "hq");
}
