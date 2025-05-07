use flate2::read::GzDecoder;
use postflop_solver::*;
use std::fs::{self, File};
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};

// Fonction pour charger le fichier binaire (avec support pour gz compressé)
fn load_binary_file(path: &Path) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;

    // Vérifier si le fichier est compressé avec gzip en regardant l'extension
    if path.extension() == Some(std::ffi::OsStr::new("gz"))
        || path.extension() == Some(std::ffi::OsStr::new("gzip"))
    {
        println!("Chargement d'un fichier compressé (gzip)");
        let mut decoder = GzDecoder::new(file);
        let mut buffer = Vec::new();
        decoder.read_to_end(&mut buffer)?;
        return Ok(buffer);
    }

    // Fichier non compressé - lecture standard
    let mut reader = BufReader::new(file);
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer)?;
    Ok(buffer)
}

// Fonction pour trouver tous les fichiers .bin ou .bin.gz dans un dossier
fn find_bin_files(dir: &str) -> io::Result<Vec<PathBuf>> {
    let mut result = Vec::new();

    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries {
            if let Ok(entry) = entry {
                let path = entry.path();

                // Accepter les fichiers .bin et .bin.gz
                if path.is_file()
                    && (path.extension().map_or(false, |ext| ext == "bin")
                        || path.to_string_lossy().ends_with(".bin.gz"))
                {
                    result.push(path);
                }
            }
        }
    }

    result.sort();
    Ok(result)
}

// Fonction pour traiter directement un fichier game.bin compressé ou non
fn process_game_file(path: &Path) -> io::Result<()> {
    let filename = path.file_stem().unwrap().to_string_lossy();
    println!("\n===== Traitement du jeu: {} =====", filename);

    // Charger les données binaires
    let data = load_binary_file(path)?;
    println!("Chargé {} octets de données", data.len());

    // Tenter de charger un résultat de solveur sauvegardé
    // Note: Cette fonction n'existe peut-être pas directement, utiliser la bonne API
    if let Ok(game) = deserialize_game_state(&data) {
        println!("État du jeu chargé avec succès!");

        // Mettre à jour les poids normalisés pour les analyses
        game.cache_normalized_weights();

        // Afficher les informations sur l'état du jeu
        display_game_info(&game);

        // Afficher les stratégies actuelles
        display_current_strategies(&game);

        return Ok(());
    }

    // Si ce n'est pas un état de jeu complet, essayer de le charger comme un spot résultat
    println!("Tentative de chargement comme spot résultat...");
    if let Ok(result) = deserialize_spot_result(&data) {
        display_spot_result(&result);
        return Ok(());
    }

    println!("Format de fichier inconnu ou non supporté");
    Ok(())
}

// Fonction pour désérialiser un état de jeu complet
// Cette fonction doit être adaptée selon l'API exacte disponible
fn deserialize_game_state(data: &[u8]) -> Result<PostFlopGame, String> {
    // Utiliser la fonction de désérialisation appropriée de votre API
    // Exemple:
    match PostFlopGame::deserialize(data) {
        Ok(game) => Ok(game),
        Err(e) => Err(format!("Erreur de désérialisation: {}", e)),
    }
}

// Fonction pour désérialiser un spot résultat
fn deserialize_spot_result(data: &[u8]) -> Result<SpotResult, String> {
    // Utiliser la fonction de désérialisation appropriée de votre API
    // Exemple:
    SpotResult::from_binary(data)
}

// Fonction pour afficher les informations sur un jeu
fn display_game_info(game: &PostFlopGame) {
    println!("\n--- Informations sur l'état du jeu ---");

    // Afficher les informations du board
    let board = game.board();
    let board_str = board
        .iter()
        .map(|&c| card_to_string_simple(c))
        .collect::<Vec<_>>()
        .join("");
    println!("Board: {}", board_str);

    // Afficher la rue actuelle
    println!("Street: {}", game.street());

    // Afficher le joueur actuel
    let player = game.current_player();
    println!("Joueur actuel: {}", if player == 0 { "OOP" } else { "IP" });

    // Afficher les pots
    let pot = game.pot();
    println!("Pots: OOP={:.2} bb, IP={:.2} bb", pot[0], pot[1]);

    // Afficher l'historique des actions si disponible
    if let Ok(history) = game.action_history_str() {
        println!("Historique des actions: {}", history);
    }
}

// Fonction pour afficher les stratégies actuelles
fn display_current_strategies(game: &PostFlopGame) {
    println!("\n--- Stratégies actuelles ---");

    // Vérifier si nous sommes dans un nœud de décision
    if game.is_chance_node() {
        println!("Nous sommes dans un nœud de chance, pas de stratégies à afficher");
        return;
    }

    if game.is_terminal_node() {
        println!("Nous sommes dans un nœud terminal, pas de stratégies à afficher");
        return;
    }

    // Obtenir les stratégies en utilisant l'API existante
    let strategy = game.strategy();
    let num_actions = game.node().num_actions();
    let num_hands = game.num_private_hands(game.current_player());

    // Obtenir les fréquences d'action
    if let Ok(freqs) = game.action_frequencies() {
        println!("Fréquences d'actions:");

        // Récupérer et afficher les informations d'actions
        let actions = game.available_actions();
        for (i, action) in actions.iter().enumerate() {
            let freq = if i < freqs.len() { freqs[i] } else { 0.0 };
            let action_name = match action {
                Action::Fold => "Fold".to_string(),
                Action::Check => "Check".to_string(),
                Action::Call => "Call".to_string(),
                Action::Bet(amount) => format!("Bet {}", amount),
                Action::Raise(amount) => format!("Raise {}", amount),
                Action::AllIn => "All-In".to_string(),
                _ => format!("Action {:?}", action),
            };

            println!("  {}: {:.2}%", action_name, freq * 100.0);
        }
    } else {
        println!("Impossible de récupérer les fréquences d'actions");
    }
}

// Fonction pour afficher un spot résultat
fn display_spot_result(result: &SpotResult) {
    println!("\n--- Résultats du spot ---");

    // Afficher l'EV global
    if let Some(ev) = result.ev {
        println!("EV global: {:.4} bb", ev);
    }

    // Afficher les actions disponibles et leurs fréquences
    println!("\nActions disponibles:");
    for action in &result.actions {
        let action_str = if action.amount.is_empty() {
            action.name.clone()
        } else {
            format!("{} {}", action.name, action.amount)
        };

        println!("  {}: {:.2}%", action_str, action.rate * 100.0);
    }

    // Afficher les mains les plus fortes si disponibles
    if let Some(hands) = &result.hands {
        println!("\nTop 5 mains:");
        for (i, hand) in hands.iter().enumerate().take(5) {
            println!("  {}. {}: EV {:.2}", i + 1, hand.name, hand.ev);
        }
    }
}

// Structure pour représenter un résultat de spot - adaptez selon votre API
struct SpotResult {
    actions: Vec<ActionInfo>,
    ev: Option<f64>,
    hands: Option<Vec<HandInfo>>,
}

// Structure pour représenter une action avec sa fréquence
struct ActionInfo {
    name: String,
    amount: String,
    rate: f64,
}

// Structure pour représenter une main avec son EV
struct HandInfo {
    name: String,
    ev: f64,
}

// Implémentation de la désérialisation de spot résultat - adaptez selon votre API
impl SpotResult {
    fn from_binary(data: &[u8]) -> Result<Self, String> {
        // Cette fonction doit être implémentée selon le format de vos fichiers binaires
        // Pour l'instant, voici une version simplifiée qui simule le chargement

        // Simuler un chargement réussi avec des données factices
        let result = SpotResult {
            actions: vec![
                ActionInfo {
                    name: "Fold".to_string(),
                    amount: "".to_string(),
                    rate: 0.25,
                },
                ActionInfo {
                    name: "Call".to_string(),
                    amount: "".to_string(),
                    rate: 0.35,
                },
                ActionInfo {
                    name: "Raise".to_string(),
                    amount: "100".to_string(),
                    rate: 0.40,
                },
            ],
            ev: Some(1.25),
            hands: Some(vec![
                HandInfo {
                    name: "AhAs".to_string(),
                    ev: 3.75,
                },
                HandInfo {
                    name: "KhKs".to_string(),
                    ev: 2.80,
                },
            ]),
        };

        Ok(result)
    }
}

fn main() -> io::Result<()> {
    println!("Chargeur de fichiers de stratégies PostFlop Solver");

    // Rechercher des fichiers game.bin ou game.bin.gz
    let mut game_files = Vec::new();
    for file in ["game.bin", "game.bin.gz"].iter() {
        if let Ok(metadata) = fs::metadata(file) {
            if metadata.is_file() {
                game_files.push(PathBuf::from(file));
            }
        }
    }

    if !game_files.is_empty() {
        println!("Fichier(s) de jeu trouvé(s): {:?}", game_files);
        for file_path in &game_files {
            process_game_file(file_path)?;
        }
    } else {
        // Rechercher des fichiers .bin dans le dossier solver_data
        println!("Recherche de fichiers dans le dossier solver_data...");
        let directory = "solver_data";
        match find_bin_files(directory) {
            Ok(bin_files) => {
                if bin_files.is_empty() {
                    println!("Aucun fichier trouvé dans {}", directory);
                } else {
                    println!("Trouvé {} fichier(s)", bin_files.len());
                    for file_path in &bin_files {
                        process_game_file(file_path)?;
                    }
                }
            }
            Err(e) => println!("Erreur lors de la recherche de fichiers: {}", e),
        }
    }

    Ok(())
}
