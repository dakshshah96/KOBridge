//! Reading stats from a local snapshot of KOReader's statistics.sqlite3.

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
pub struct Day { pub date: String, pub minutes: f64 }

#[derive(Serialize)]
pub struct Book {
    pub title: String,
    pub authors: String,
    pub pages: i64,
    pub percent: f64,
    pub hours: f64,
    pub last_open: i64,
}

#[derive(Serialize)]
pub struct Stats {
    pub total_hours: f64,
    pub days_read: i64,
    pub books: i64,
    pub avg_minutes: f64,
    pub snapshot_at: i64,
    pub daily: Vec<Day>,
    pub recent: Vec<Book>,
}

pub fn read(db: &Path, snapshot_at: i64) -> rusqlite::Result<Stats> {
    let c = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let (secs, days_read, books): (f64, i64, i64) = c.query_row(
        "SELECT coalesce(sum(duration),0), count(DISTINCT date(start_time,'unixepoch','localtime')), count(DISTINCT id_book) FROM page_stat_data",
        [], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;

    // Last 30 calendar days, zero-filled so the chart has one bar slot per day.
    let mut q = c.prepare(
        "WITH RECURSIVE d(n) AS (SELECT 0 UNION ALL SELECT n+1 FROM d WHERE n < 29)
         SELECT date('now','localtime','-' || (29-n) || ' days') AS day,
                coalesce((SELECT sum(duration) FROM page_stat_data
                          WHERE date(start_time,'unixepoch','localtime') = date('now','localtime','-' || (29-n) || ' days')), 0) / 60.0
         FROM d ORDER BY n")?;
    let daily = q.query_map([], |r| Ok(Day { date: r.get(0)?, minutes: r.get(1)? }))?.collect::<Result<_, _>>()?;

    let mut q = c.prepare(
        "SELECT b.title, replace(coalesce(b.authors,''), char(10), ', '), coalesce(b.pages,0),
                coalesce((SELECT max(1.0*page/total_pages) FROM page_stat_data p WHERE p.id_book=b.id AND total_pages>0), 0),
                coalesce(b.total_read_time,0)/3600.0, coalesce(b.last_open,0)
         FROM book b ORDER BY b.last_open DESC LIMIT 12")?;
    let recent = q.query_map([], |r| Ok(Book {
        title: r.get(0)?, authors: r.get(1)?, pages: r.get(2)?, percent: r.get(3)?, hours: r.get(4)?, last_open: r.get(5)?,
    }))?.collect::<Result<_, _>>()?;

    Ok(Stats {
        total_hours: secs / 3600.0,
        days_read,
        books,
        avg_minutes: if days_read > 0 { secs / 60.0 / days_read as f64 } else { 0.0 },
        snapshot_at,
        daily,
        recent,
    })
}
