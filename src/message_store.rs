use rusqlite::{params, Connection, Result};
use std::sync::Mutex;

pub struct MessageStore {
    conn: Mutex<Connection>,
}

impl MessageStore {
    pub fn new() -> Result<Self> {
        let conn = Connection::open("chat.db")?;
        conn.execute(
            "CREATE TABLE IF NOT EXISTS messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                room_id TEXT NOT NULL,
                room_name TEXT NOT NULL,
                message TEXT NOT NULL
            );",
            [],
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn add_message(&self, room_id: String, room_name: String, message: String) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO messages (room_id, room_name, message) VALUES (?1, ?2, ?3)",
            params![room_id, room_name, message],
        )?;
        Ok(())
    }

    pub fn get_messages(&self, room_id: &str) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT message FROM messages WHERE room_id = ?1")?;
        let messages_iter = stmt.query_map(params![room_id], |row| {
            Ok(row.get(0)?)
        })?;

        let mut messages = Vec::new();
        for message in messages_iter {
            messages.push(message?);
        }
        Ok(messages)
    }

    pub fn get_rooms(&self) -> Result<Vec<(String, String)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT DISTINCT room_id, room_name FROM messages")?;
        let room_names_iter = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
    
        let mut rooms = Vec::new();
        for room in room_names_iter {
            rooms.push(room?);
        }
        Ok(rooms)
    }

    // Function to add a new room (if your app needs to support this operation)
    pub fn add_room(&self, room_id: String, room_name: String) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO messages (room_id, room_name, message) VALUES (?1, ?2, ?3)",
            params![room_id, room_name, ""],
        )?;
        Ok(())
    }
}
