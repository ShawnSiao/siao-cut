//! A rollback-on-drop write scope that supports composing existing domain operations.
//! The outer application command owns BEGIN IMMEDIATE; domain scopes use savepoints.
use anyhow::Result;
use rusqlite::Connection;
use std::ops::{Deref, DerefMut};

pub struct WriteTransaction<'a> {
    db: &'a mut Connection,
    savepoint: Option<String>,
    completed: bool,
}
impl<'a> WriteTransaction<'a> {
    pub fn begin(db: &'a mut Connection) -> Result<Self> {
        let savepoint = if db.is_autocommit() {
            None
        } else {
            Some(format!("sc_{}", uuid::Uuid::new_v4().simple()))
        };
        match &savepoint {
            Some(name) => db.execute_batch(&format!("SAVEPOINT {name}"))?,
            None => db.execute_batch("BEGIN IMMEDIATE")?,
        }
        Ok(Self {
            db,
            savepoint,
            completed: false,
        })
    }
    pub fn commit(mut self) -> Result<()> {
        match &self.savepoint {
            Some(name) => self
                .db
                .execute_batch(&format!("RELEASE SAVEPOINT {name}"))?,
            None => self.db.execute_batch("COMMIT")?,
        }
        self.completed = true;
        Ok(())
    }
}
impl Deref for WriteTransaction<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.db
    }
}
impl DerefMut for WriteTransaction<'_> {
    fn deref_mut(&mut self) -> &mut Connection {
        self.db
    }
}
impl Drop for WriteTransaction<'_> {
    fn drop(&mut self) {
        if !self.completed {
            let _ = match &self.savepoint {
                Some(name) => self.db.execute_batch(&format!(
                    "ROLLBACK TO SAVEPOINT {name}; RELEASE SAVEPOINT {name}"
                )),
                None => self.db.execute_batch("ROLLBACK"),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inner_commit_does_not_escape_outer_rollback() {
        let mut db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE entries(value TEXT)")
            .unwrap();
        {
            let mut outer = WriteTransaction::begin(&mut db).unwrap();
            let inner = WriteTransaction::begin(&mut outer).unwrap();
            inner
                .execute("INSERT INTO entries VALUES('inner')", [])
                .unwrap();
            inner.commit().unwrap();
        }
        let count: i64 = db
            .query_row("SELECT count(*) FROM entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 0);
        assert!(db.is_autocommit());
    }
}
