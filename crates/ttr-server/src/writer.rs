//! Writer di salvataggio per partita: serializza le scritture su disco fuori
//! dall'attore, in ordine, con `spawn_blocking`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::save::{GameResult, SaveData, append_result, write_atomic};

/// Lavoro per il writer.
enum WriteJob {
    /// Stato corrente della partita.
    Save(Box<SaveData>),
    /// Partita conclusa: archivia il risultato, poi elimina il save. Se
    /// l'archiviazione fallisce il save finale resta su disco.
    Archive {
        result: Box<GameResult>,
        final_save: Box<SaveData>,
    },
}

/// Task che scrive il save di una partita. Tra i lavori già in coda conta
/// solo l'ultimo salvataggio; l'archiviazione chiude la sequenza.
pub(crate) struct SaveWriter {
    tx: mpsc::UnboundedSender<WriteJob>,
    task: JoinHandle<()>,
}

impl SaveWriter {
    pub fn spawn(path: PathBuf, results_lock: Arc<Mutex<()>>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let task = tokio::spawn(writer_loop(path, results_lock, rx));
        Self { tx, task }
    }

    /// Accoda uno snapshot del save.
    pub fn save(&self, data: SaveData) {
        let _ = self.tx.send(WriteJob::Save(Box::new(data)));
    }

    /// Accoda l'archiviazione del risultato e l'eliminazione del save.
    pub fn archive(&self, result: GameResult, final_save: SaveData) {
        let _ = self.tx.send(WriteJob::Archive {
            result: Box::new(result),
            final_save: Box::new(final_save),
        });
    }

    /// Chiude la coda e attende che i lavori pendenti siano su disco.
    pub async fn finish(self) {
        drop(self.tx);
        let _ = self.task.await;
    }
}

async fn writer_loop(
    path: PathBuf,
    results_lock: Arc<Mutex<()>>,
    mut rx: mpsc::UnboundedReceiver<WriteJob>,
) {
    while let Some(mut job) = rx.recv().await {
        while let Ok(next) = rx.try_recv() {
            if !matches!(job, WriteJob::Archive { .. }) {
                job = next;
            }
        }
        let (path, lock) = (path.clone(), results_lock.clone());
        if let Err(e) = tokio::task::spawn_blocking(move || run_job(&path, &lock, job)).await {
            tracing::error!(error = %e, "task di scrittura save terminato in modo anomalo");
        }
    }
}

fn run_job(path: &Path, lock: &Mutex<()>, job: WriteJob) {
    match job {
        WriteJob::Save(data) => {
            if let Err(e) = write_atomic(path, &data) {
                tracing::error!(game_id = %data.game_id, path = %path.display(), error = %e, "scrittura save fallita");
            }
        }
        WriteJob::Archive { result, final_save } => {
            let saves_dir = path.parent().unwrap_or_else(|| Path::new("."));
            match append_result(saves_dir, lock, &result) {
                Ok(_) => {
                    if let Err(e) = std::fs::remove_file(path)
                        && e.kind() != std::io::ErrorKind::NotFound
                    {
                        tracing::warn!(game_id = %result.game_id, error = %e, "eliminazione save fallita");
                    }
                    tracing::info!(game_id = %result.game_id, "partita conclusa: risultato archiviato, save eliminato");
                }
                Err(e) => {
                    tracing::error!(game_id = %result.game_id, error = %e, "archiviazione risultato fallita: il save resta su disco");
                    if let Err(e) = write_atomic(path, &final_save) {
                        tracing::error!(game_id = %result.game_id, error = %e, "scrittura save finale fallita");
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::save::{FORMAT_VERSION, ResultPlayer, SaveData, read_results};

    fn save_with_actions(n: usize) -> SaveData {
        SaveData {
            format_version: FORMAT_VERSION,
            rules_version: ttr_core::RULES_VERSION,
            game_id: "g".into(),
            game_name: "g".into(),
            map_id: "usa".into(),
            ruleset_id: "usa_v2".into(),
            seed: n as u64,
            first_player: 0,
            players: vec![],
            actions: vec![],
            status: "in_progress".into(),
            created_at: "t".into(),
            updated_at: "t".into(),
        }
    }

    fn result() -> GameResult {
        GameResult {
            game_id: "g".into(),
            game_name: "g".into(),
            ruleset_id: "usa_v2".into(),
            finished_at: "t".into(),
            players: vec![ResultPlayer {
                name: "Anna".into(),
                total: 1,
                winner: true,
            }],
        }
    }

    #[tokio::test]
    async fn last_save_wins_and_finish_flushes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("g.json");
        let writer = SaveWriter::spawn(path.clone(), Arc::default());
        for n in 0..50 {
            writer.save(save_with_actions(n));
        }
        writer.finish().await;
        let saved = crate::save::read_save(&path).unwrap();
        assert_eq!(saved.seed, 49, "l'ultimo snapshot è quello su disco");
    }

    #[tokio::test]
    async fn archive_appends_result_then_deletes_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("g.json");
        let writer = SaveWriter::spawn(path.clone(), Arc::default());
        writer.save(save_with_actions(1));
        writer.archive(result(), save_with_actions(2));
        writer.finish().await;
        assert!(!path.exists(), "save eliminato dopo l'archiviazione");
        assert_eq!(read_results(dir.path()).len(), 1);
    }

    #[tokio::test]
    async fn failed_archive_keeps_final_save() {
        let dir = tempfile::tempdir().unwrap();
        // `results.jsonl` è una directory: l'append fallisce.
        std::fs::create_dir(dir.path().join(crate::save::RESULTS_FILE)).unwrap();
        let path = dir.path().join("g.json");
        let writer = SaveWriter::spawn(path.clone(), Arc::default());
        writer.archive(result(), save_with_actions(2));
        writer.finish().await;
        assert_eq!(crate::save::read_save(&path).unwrap().seed, 2);
    }
}
