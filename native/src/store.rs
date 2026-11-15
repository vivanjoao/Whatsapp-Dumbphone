use crate::auth::{digest, now};
use anyhow::Result;
use diesel::{
    prelude::*,
    r2d2::{ConnectionManager, Pool},
    sql_types::{BigInt, Text},
};
use std::{path::Path, time::Duration};

#[derive(QueryableByName)]
struct Count {
    #[diesel(sql_type = BigInt)]
    value: i64,
}
#[derive(Clone)]
pub struct Store {
    pool: Pool<ConnectionManager<SqliteConnection>>,
}
impl Store {
    pub async fn open(path: &Path) -> Result<Self> {
        let path = path.to_string_lossy().into_owned();
        let pool = tokio::task::spawn_blocking(move || {
            Pool::builder()
                .max_size(4)
                .connection_timeout(Duration::from_secs(5))
                .build(ConnectionManager::new(path))
        })
        .await??;
        let store = Self { pool };
        store.run(|conn| {
            for sql in [
                "PRAGMA journal_mode=WAL", "PRAGMA busy_timeout=5000",
                "CREATE TABLE IF NOT EXISTS sessions(id TEXT PRIMARY KEY, epoch TEXT NOT NULL, expires INTEGER NOT NULL)",
                "CREATE INDEX IF NOT EXISTS session_expiry ON sessions(expires)",
                "CREATE TABLE IF NOT EXISTS submissions(id TEXT PRIMARY KEY, state TEXT NOT NULL, created INTEGER NOT NULL)",
                "CREATE INDEX IF NOT EXISTS submission_age ON submissions(created)",
                "CREATE TABLE IF NOT EXISTS attempts(ip TEXT PRIMARY KEY, count INTEGER NOT NULL, expires INTEGER NOT NULL)",
                "UPDATE submissions SET state='unknown' WHERE state='pending'",
            ] { diesel::sql_query(sql).execute(conn)?; } Ok(())
        }).await?;
        Ok(store)
    }
    async fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut SqliteConnection) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let mut conn = pool.get()?;
            diesel::sql_query("PRAGMA busy_timeout=5000").execute(&mut conn)?;
            diesel::sql_query("PRAGMA synchronous=FULL").execute(&mut conn)?;
            f(&mut conn)
        })
        .await?
    }
    pub async fn session(&self, id: &str, epoch: &str) -> Result<bool> {
        let id = digest(id);
        let epoch = epoch.to_owned();
        self.run(move |conn| {
            Ok(diesel::sql_query(
                "SELECT COUNT(*) AS value FROM sessions WHERE id=? AND epoch=? AND expires>?",
            )
            .bind::<Text, _>(id)
            .bind::<Text, _>(epoch)
            .bind::<BigInt, _>(now())
            .get_result::<Count>(conn)?
            .value
                == 1)
        })
        .await
    }
    pub async fn login(&self, id: &str, epoch: &str) -> Result<()> {
        let id = digest(id);
        let epoch = epoch.to_owned();
        self.run(move |conn| {
            diesel::sql_query("INSERT INTO sessions VALUES(?,?,?)")
                .bind::<Text, _>(id)
                .bind::<Text, _>(epoch)
                .bind::<BigInt, _>(now() + 30 * 86400)
                .execute(conn)?;
            Ok(())
        })
        .await
    }
    pub async fn revoke(&self, id: &str) -> Result<()> {
        let id = digest(id);
        self.run(move |conn| {
            diesel::sql_query("DELETE FROM sessions WHERE id=?")
                .bind::<Text, _>(id)
                .execute(conn)?;
            Ok(())
        })
        .await
    }
    pub async fn attempt(&self, ip: &str) -> Result<bool> {
        let ip = digest(ip);
        self.run(move |conn| conn.immediate_transaction::<_,anyhow::Error,_>(|conn| {
            diesel::sql_query("DELETE FROM attempts WHERE expires<=?").bind::<BigInt,_>(now()).execute(conn)?;
            let existing = diesel::sql_query("SELECT COUNT(*) AS value FROM attempts WHERE ip=?").bind::<Text,_>(&ip).get_result::<Count>(conn)?.value > 0;
            let size = diesel::sql_query("SELECT COUNT(*) AS value FROM attempts").get_result::<Count>(conn)?.value;
            let key = if !existing && size >= 1024 { "overflow".to_string() } else { ip };
            diesel::sql_query("INSERT INTO attempts VALUES(?,1,?) ON CONFLICT(ip) DO UPDATE SET count=count+1").bind::<Text,_>(&key).bind::<BigInt,_>(now()+900).execute(conn)?;
            Ok(diesel::sql_query("SELECT count AS value FROM attempts WHERE ip=?").bind::<Text,_>(key).get_result::<Count>(conn)?.value <= 5)
        })).await
    }
    pub async fn reserve(&self, id: &str) -> Result<bool> {
        let id = digest(id);
        self.run(move |conn| {
            Ok(
                diesel::sql_query("INSERT OR IGNORE INTO submissions VALUES(?,'pending',?)")
                    .bind::<Text, _>(id)
                    .bind::<BigInt, _>(now())
                    .execute(conn)?
                    == 1,
            )
        })
        .await
    }
    pub async fn finish(&self, id: &str, sent: bool) -> Result<()> {
        let id = digest(id);
        self.run(move |conn| {
            diesel::sql_query("UPDATE submissions SET state=? WHERE id=?")
                .bind::<Text, _>(if sent { "sent" } else { "unknown" })
                .bind::<Text, _>(id)
                .execute(conn)?;
            Ok(())
        })
        .await
    }
    pub async fn prune(&self) -> Result<()> {
        self.run(|conn| {
            diesel::sql_query("DELETE FROM sessions WHERE expires<=?")
                .bind::<BigInt, _>(now())
                .execute(conn)?;
            diesel::sql_query("DELETE FROM attempts WHERE expires<=?")
                .bind::<BigInt, _>(now())
                .execute(conn)?;
            // Signatures expire in one hour; IDs remain for two days.
            diesel::sql_query("DELETE FROM submissions WHERE created<?")
                .bind::<BigInt, _>(now() - 2 * 86400)
                .execute(conn)?;
            Ok(())
        })
        .await
    }
}
