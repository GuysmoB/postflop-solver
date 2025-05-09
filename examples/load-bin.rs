use flate2::read::GzDecoder;
use postflop_solver::*;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

#[derive(Default, Debug)]
struct GameData {
    board: Vec<String>,
    pot_oop: f64,
    pot_ip: f64,
    current_player: usize,
    oop_hands: Vec<String>,
    oop_cards: Vec<(u8, u8)>,
    oop_weights: Vec<f64>,
    oop_equity: Vec<f64>,
    oop_ev: Vec<f64>,
    ip_hands: Vec<String>,
    ip_cards: Vec<(u8, u8)>,
    ip_weights: Vec<f64>,
    ip_equity: Vec<f64>,
    ip_ev: Vec<f64>,
    actions: Vec<String>,
    strategy: Vec<f64>,
    action_ev: Vec<f64>,
}

fn main() -> io::Result<()> {
    // Spécifier le répertoire où se trouvent les fichiers .txt
    let directory = "solver_results";
    let path = Path::new(directory).join("F_Bet10.txt");

    if path.exists() {
        println!("Chargement du fichier: {}", path.display());
        match load_text_file(&path) {
            Ok(data) => {
                println!("Fichier chargé avec succès");
                display_game_data(&data)?;
            }
            Err(e) => println!("Erreur lors du chargement: {}", e),
        }
    } else {
        println!("Le fichier {} n'existe pas", path.display());
    }

    Ok(())
}

fn deserialize_direct_game_data(data: &[u8]) -> Result<GameData, String> {
    // Vérifier la taille minimale du buffer pour l'en-tête
    if data.len() < 6 {
        return Err("Buffer too small for header".to_string());
    }

    // Les 6 premiers octets devraient être "GDATA1"
    let header = &data[0..6];
    if header != b"GDATA1" {
        return Err(format!(
            "Invalid header: {:?}, expected: GDATA1",
            std::str::from_utf8(header).unwrap_or("invalid utf-8")
        ));
    }

    let mut result = GameData::default();
    let mut offset = 6;

    // Lire la taille du board
    if offset + 4 > data.len() {
        return Err("Buffer too small for board size".to_string());
    }

    let board_size = u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]) as usize;
    offset += 4;

    // Lire les cartes du board
    // for _ in 0..board_size {
    //     if offset + 4 > data.len() {
    //         return Err("Buffer too small for board card".to_string());
    //     }

    //     let card = u32::from_le_bytes([
    //         data[offset],
    //         data[offset + 1],
    //         data[offset + 2],
    //         data[offset + 3],
    //     ]) as u8;
    //     result.board.push(card);
    //     offset += 4;
    // }

    // Lire pot_oop
    if offset + 4 > data.len() {
        return Err("Buffer too small for pot_oop".to_string());
    }

    result.pot_oop = u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]) as f64;
    offset += 4;

    // Lire pot_ip
    if offset + 4 > data.len() {
        return Err("Buffer too small for pot_ip".to_string());
    }

    result.pot_ip = u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]) as f64;
    offset += 4;

    // Lire current_player
    if offset + 4 > data.len() {
        return Err("Buffer too small for current_player".to_string());
    }

    result.current_player = u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]) as usize;
    offset += 4;

    // Lire les données pour chaque joueur (0=OOP, 1=IP)
    for player in 0..2 {
        // Lire le nombre de mains
        if offset + 4 > data.len() {
            return Err(format!(
                "Buffer too small for hands count of player {}",
                player
            ));
        }

        let num_hands = u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]) as usize;
        offset += 4;

        // Sélectionner les vecteurs appropriés selon le joueur
        let cards = if player == 0 {
            &mut result.oop_cards
        } else {
            &mut result.ip_cards
        };
        let weights = if player == 0 {
            &mut result.oop_weights
        } else {
            &mut result.ip_weights
        };
        let equity = if player == 0 {
            &mut result.oop_equity
        } else {
            &mut result.ip_equity
        };
        let ev = if player == 0 {
            &mut result.oop_ev
        } else {
            &mut result.ip_ev
        };

        // Lire les données pour chaque main
        for _ in 0..num_hands {
            // Lire card1
            if offset + 4 > data.len() {
                return Err(format!("Buffer too small for card1 of player {}", player));
            }

            let card1 = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]) as u8;
            offset += 4;

            // Lire card2
            if offset + 4 > data.len() {
                return Err(format!("Buffer too small for card2 of player {}", player));
            }

            let card2 = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]) as u8;
            offset += 4;

            // Ajouter la paire de cartes
            cards.push((card1, card2));

            // Lire weight
            if offset + 8 > data.len() {
                return Err(format!("Buffer too small for weight of player {}", player));
            }

            let weight = f64::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]);
            weights.push(weight);
            offset += 8;

            // Lire equity
            if offset + 8 > data.len() {
                return Err(format!("Buffer too small for equity of player {}", player));
            }

            let eq = f64::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]);
            equity.push(eq);
            offset += 8;

            // Lire EV
            if offset + 8 > data.len() {
                return Err(format!("Buffer too small for EV of player {}", player));
            }

            let ev_val = f64::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
                data[offset + 4],
                data[offset + 5],
                data[offset + 6],
                data[offset + 7],
            ]);
            ev.push(ev_val);
            offset += 8;

            // Lire la longueur du nom de la main
            if offset + 4 > data.len() {
                return Err(format!(
                    "Buffer too small for hand name length of player {}",
                    player
                ));
            }

            let name_len = u32::from_le_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]) as usize;
            offset += 4;

            // Sauter le nom de la main (on utilisera format_hand_cards à l'affichage)
            if offset + name_len > data.len() {
                return Err(format!(
                    "Buffer too small for hand name of player {}",
                    player
                ));
            }

            offset += name_len;
        }
    }

    // Le fichier est correctement chargé
    Ok(result)
}
fn process_game_file(path: &Path) -> io::Result<()> {
    let filename = path.file_stem().unwrap().to_string_lossy();
    println!("\n===== Processing file: {} =====", filename);

    // Load binary data
    let data = load_binary_file(path)?;
    println!("Loaded {} bytes of data", data.len());

    // Try to load as direct game data first
    if let Ok(game_data) = deserialize_direct_game_data(&data) {
        println!("Successfully loaded direct game data");
        display_game_data(&game_data);
        return Ok(());
    }

    println!("Not a recognized file format");
    Ok(())
}

/// Finds all .bin files in the specified directory
fn find_bin_files(directory: &str) -> io::Result<Vec<PathBuf>> {
    let mut bin_files = Vec::new();

    if !Path::new(directory).exists() {
        return Ok(bin_files);
    }

    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() && path.extension().map_or(false, |ext| ext == "bin") {
            bin_files.push(path);
        }
    }

    Ok(bin_files)
}

/// Loads binary data from a file, handling both regular and gzipped files
fn load_binary_file(path: &Path) -> io::Result<Vec<u8>> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);

    // Check if this is a gzipped file
    let mut header = [0; 2];
    if reader.read_exact(&mut header).is_ok() && header == [0x1f, 0x8b] {
        // Reset the file position
        let file = File::open(path)?;
        let mut decoder = GzDecoder::new(file);
        let mut buffer = Vec::new();
        decoder.read_to_end(&mut buffer)?;
        Ok(buffer)
    } else {
        // Not gzipped, read normally
        let file = File::open(path)?;
        let mut buffer = Vec::new();
        BufReader::new(file).read_to_end(&mut buffer)?;
        Ok(buffer)
    }
}

fn get_action_names(result: &SpecificResultData, num_actions: usize) -> Vec<String> {
    let mut action_names = Vec::new();

    // En fonction du nombre d'actions, déduire les types d'action
    match num_actions {
        0 => {}
        1 => {
            action_names.push("Check/Call".to_string());
        }
        2 => {
            // Cas typique: Fold/Check vs Call/Bet
            if result.current_player == "oop" {
                action_names.push("Check".to_string());
                action_names.push("Bet".to_string());
            } else {
                action_names.push("Fold".to_string());
                action_names.push("Call".to_string());
            }
        }
        3 => {
            // Cas typique: Fold vs Call vs Raise
            if result.current_player == "oop" {
                action_names.push("Fold".to_string());
                action_names.push("Call".to_string());
                action_names.push("Raise".to_string());
            } else {
                action_names.push("Fold".to_string());
                action_names.push("Call".to_string());
                action_names.push("Raise".to_string());
            }
        }
        _ => {
            // Pour les cas avec plus d'actions, utiliser Fold, Call, puis des tailles de mise croissantes
            action_names.push("Fold".to_string());
            action_names.push("Call".to_string());

            // Ajouter des tailles de mise en fonction du nombre d'actions restantes
            for i in 2..num_actions {
                let bet_size = match i {
                    2 => "Min bet",
                    3 => "1/3 pot",
                    4 => "1/2 pot",
                    5 => "2/3 pot",
                    6 => "Pot",
                    7 => "2x pot",
                    _ => "Large bet",
                };
                action_names.push(bet_size.to_string());
            }
        }
    }

    action_names
}

// Fonctions pour convertir les rangs et couleurs en caractères lisibles
fn rank_to_char(rank: usize) -> char {
    match rank {
        12 => 'A',
        11 => 'K',
        10 => 'Q',
        9 => 'J',
        8 => 'T',
        _ => ('2' as u8 + rank as u8) as char,
    }
}

fn suit_to_char(suit: usize) -> char {
    match suit {
        0 => 's', // spades
        1 => 'h', // hearts
        2 => 'd', // diamonds
        3 => 'c', // clubs
        _ => '?',
    }
}

fn display_top_hands_from_file(result: &SpecificResultData, num_hands: usize) {
    println!("\n--- DÉTAIL DES MEILLEURES MAINS ---");

    // Pour chaque joueur (OOP=0, IP=1)
    for player_idx in 0..2 {
        let player_label = if player_idx == 0 { "OOP" } else { "IP" };
        println!("\n{} - Meilleures mains:", player_label);

        // Vérifier si les données sont disponibles pour ce joueur
        if result.equity[player_idx].is_empty() || result.ev[player_idx].is_empty() {
            println!("Aucune donnée disponible pour {}", player_label);
            continue;
        }

        // Obtenir les cartes pour ce joueur
        let hand_cards = if player_idx == 0 {
            &result.oop_cards
        } else {
            &result.ip_cards
        };

        // Créer une structure pour stocker et trier les données des mains
        struct HandData {
            hand_index: usize,
            equity: f64,
            ev: f64,
            weight: f64,
        }

        // Collecter les données pour toutes les mains avec un poids > 0
        let mut hand_data: Vec<HandData> = (0..result.equity[player_idx].len())
            .filter(|&i| result.normalizer[player_idx][i] > 0.0)
            .map(|i| HandData {
                hand_index: i,
                equity: result.equity[player_idx][i],
                ev: result.ev[player_idx][i],
                weight: result.normalizer[player_idx][i],
            })
            .collect();

        // Trier les mains par EV décroissant
        hand_data.sort_by(|a, b| b.ev.partial_cmp(&a.ev).unwrap_or(std::cmp::Ordering::Equal));

        // Afficher l'en-tête du tableau
        println!(
            "{:<6} {:<10} {:<12} {:<10}",
            "Main", "Équité %", "EV (bb)", "Poids %"
        );
        println!("{}", "-".repeat(40));

        // Afficher les N meilleures mains
        for data in hand_data.iter().take(num_hands) {
            // Formater le nom de la main si les cartes sont disponibles
            let hand_name = if data.hand_index < hand_cards.len() {
                format_hand_cards(hand_cards[data.hand_index])
            } else {
                format!("Hand{}", data.hand_index)
            };

            println!(
                "{:<6} {:<10.2} {:<12.2} {:<10.2}",
                hand_name,
                data.equity * 100.0,
                data.ev,
                data.weight * 100.0
            );
        }

        // Calculer et afficher l'EV moyenne
        let total_ev: f64 = hand_data.iter().map(|data| data.ev * data.weight).sum();
        let total_weight: f64 = hand_data.iter().map(|data| data.weight).sum();
        let avg_ev = if total_weight > 0.0 {
            total_ev / total_weight
        } else {
            0.0
        };

        println!("\nEV moyenne {}: {:.2} bb", player_label, avg_ev);
    }

    // Afficher les fréquences d'actions si disponibles
    if !result.strategy.is_empty() {
        display_action_frequencies(result);
    }
}

fn display_action_frequencies(result: &SpecificResultData) {
    println!("\n--- FRÉQUENCES D'ACTIONS ---");

    // Déterminer le joueur actuel
    let player_idx = if result.current_player == "oop" { 0 } else { 1 };
    let range_size = result.equity[player_idx].len();

    if range_size == 0 {
        println!("Aucune donnée de stratégie disponible");
        return;
    }

    // Calculer la fréquence moyenne pour chaque action
    let mut action_frequencies = Vec::new();

    for action_idx in 0..result.num_actions {
        let mut total_frequency = 0.0;
        let mut total_weight = 0.0;

        for hand_idx in 0..range_size {
            let strategy_idx = action_idx * range_size + hand_idx;
            if strategy_idx < result.strategy.len() {
                let frequency = result.strategy[strategy_idx];
                let weight = result.normalizer[player_idx][hand_idx];

                total_frequency += frequency * weight;
                total_weight += weight;
            }
        }

        let avg_frequency = if total_weight > 0.0 {
            total_frequency / total_weight
        } else {
            0.0
        };

        action_frequencies.push(avg_frequency);
    }

    // Afficher les noms d'actions et leurs fréquences
    println!("{:<15} {:<10}", "Action", "Frequency %");
    println!("{}", "-".repeat(30));

    // Obtenir les noms d'actions en fonction du contexte
    let action_names = get_action_names(result, result.num_actions);

    for (action_idx, &frequency) in action_frequencies.iter().enumerate() {
        let action_name = if action_idx < action_names.len() {
            &action_names[action_idx]
        } else {
            "Action inconnue"
        };
        println!("{:<15} {:<10.2}", action_name, frequency * 100.0);
    }
}

fn display_game_data(data: &GameData) -> io::Result<()> {
    println!("\n=== INFORMATIONS DE BASE ===");
    println!("Board: {}", data.board.join(" "));
    println!("Pot OOP: {:.2} bb", data.pot_oop);
    println!("Pot IP: {:.2} bb", data.pot_ip);
    println!(
        "Joueur actuel: {}",
        if data.current_player == 0 {
            "OOP"
        } else {
            "IP"
        }
    );

    // Utiliser la fonction display_loaded_hands qui fonctionne correctement
    if let Err(e) = display_loaded_hands(data, 10, "ÉTAT ACTUEL") {
        println!("Erreur lors de l'affichage des mains: {}", e);
    }

    // Afficher la stratégie si disponible
    if !data.strategy.is_empty() && !data.actions.is_empty() {
        println!("\n=== STRATÉGIE ===");

        // Calculer et afficher les fréquences moyennes pour chaque action
        let player = data.current_player;
        let range_size = if player == 0 {
            data.oop_hands.len()
        } else {
            data.ip_hands.len()
        };
        let weights = if player == 0 {
            &data.oop_weights
        } else {
            &data.ip_weights
        };

        println!("{:<15} {:<10}", "Action", "Fréquence %");
        println!("{}", "-".repeat(30));

        let num_actions = data.actions.len();
        for action_idx in 0..num_actions {
            let mut total_freq = 0.0;
            let mut total_weight = 0.0;

            for hand_idx in 0..range_size {
                let strat_idx = action_idx * range_size + hand_idx;
                if strat_idx < data.strategy.len() {
                    total_freq += data.strategy[strat_idx] * weights[hand_idx];
                    total_weight += weights[hand_idx];
                }
            }

            let avg_freq = if total_weight > 0.0 {
                total_freq / total_weight
            } else {
                0.0
            };
            println!(
                "{:<15} {:<10.2}%",
                data.actions[action_idx],
                avg_freq * 100.0
            );
        }
    }

    Ok(())
}

fn get_default_action_names(num_actions: usize) -> Vec<String> {
    match num_actions {
        0 => Vec::new(),
        1 => vec!["Check/Call".to_string()],
        2 => vec!["Fold/Check".to_string(), "Call/Bet".to_string()],
        3 => vec![
            "Fold".to_string(),
            "Check/Call".to_string(),
            "Bet/Raise".to_string(),
        ],
        _ => {
            let mut names = vec!["Fold".to_string(), "Check/Call".to_string()];
            for i in 2..num_actions {
                names.push(format!("Bet/Raise {}", i - 1));
            }
            names
        }
    }
}

fn format_hand(card_pair: (u8, u8)) -> String {
    format!(
        "{}{}{}{}",
        rank_to_char((card_pair.0 % 13) as usize),
        suit_to_char((card_pair.0 / 13) as usize),
        rank_to_char((card_pair.1 % 13) as usize),
        suit_to_char((card_pair.1 / 13) as usize)
    )
}

fn display_loaded_hands(data: &GameData, num_hands: usize, title: &str) -> Result<(), String> {
    println!("\n--- DÉTAIL DES MEILLEURES MAINS ({}) ---", title);

    // Pour chaque joueur (OOP=0, IP=1)
    for player in 0..2 {
        let player_label = if player == 0 { "OOP" } else { "IP" };
        println!("\n{} - Meilleures mains:", player_label);

        // Récupérer les données du joueur
        let weights = if player == 0 {
            &data.oop_weights
        } else {
            &data.ip_weights
        };
        let equity = if player == 0 {
            &data.oop_equity
        } else {
            &data.ip_equity
        };
        let ev = if player == 0 {
            &data.oop_ev
        } else {
            &data.ip_ev
        };
        let cards = if player == 0 {
            &data.oop_cards
        } else {
            &data.ip_cards
        };

        println!("cards: {:?}", cards);

        // Créer une structure pour trier les mains
        struct HandData {
            hand_name: String,
            equity: f64,
            ev: f64,
            weight: f64,
        }

        // Collecter les données pour les mains avec un poids > 0
        let mut hand_data = Vec::new();

        // Vérifier que toutes les arrays ont la même taille
        let min_len = weights
            .len()
            .min(equity.len())
            .min(ev.len())
            .min(cards.len());

        for i in 0..min_len {
            if weights[i] > 0.0001 {
                // Filtrer les mains avec un poids significatif
                let hand_name = format_hand_cards(cards[i]);
                hand_data.push(HandData {
                    hand_name,
                    equity: equity[i],
                    ev: ev[i],
                    weight: weights[i],
                });
            }
        }

        // Trier les mains par EV décroissant
        hand_data.sort_by(|a, b| b.ev.partial_cmp(&a.ev).unwrap_or(std::cmp::Ordering::Equal));

        // Afficher l'en-tête du tableau
        println!(
            "{:<6} {:<10} {:<12} {:<10}",
            "Main", "Équité %", "EV (bb)", "Poids %"
        );
        println!("{}", "-".repeat(40));

        // Afficher les N meilleures mains
        for data in hand_data.iter().take(num_hands) {
            println!(
                "{:<6} {:<10.2} {:<12.2} {:<10.2}",
                data.hand_name,
                data.equity * 100.0,
                data.ev,
                data.weight
            );
        }

        // Afficher un message si nous n'affichons pas toutes les mains
        if hand_data.len() > num_hands {
            println!("... et {} autres mains", hand_data.len() - num_hands);
        }

        // Afficher l'EV moyenne du joueur
        let total_ev: f64 = hand_data.iter().map(|d| d.ev * d.weight).sum();
        let total_weight: f64 = hand_data.iter().map(|d| d.weight).sum();
        let avg_ev = if total_weight > 0.0 {
            total_ev / total_weight
        } else {
            0.0
        };

        println!("\nEV moyenne {}: {:.2} bb", player_label, avg_ev);
    }

    Ok(())
}

fn load_text_file(path: &Path) -> io::Result<GameData> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut data = GameData::default();

    let mut current_section = "";
    let mut current_player = 0;
    let mut reading_strategy = false;
    let mut reading_action_ev = false;
    let mut range_size = 0;

    for line in reader.lines() {
        let line = line?;
        let line = line.trim();

        // Ignorer les lignes vides et les commentaires
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Détecter les sections
        if line.starts_with("PLAYER: OOP") {
            current_player = 0;
            continue;
        } else if line.starts_with("PLAYER: IP") {
            current_player = 1;
            continue;
        } else if line.starts_with("STRATEGY") {
            current_section = "strategy";
            continue;
        }

        // Parse les paires clé-valeur
        if line.contains(':') {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() == 2 {
                let key = parts[0].trim();
                let value = parts[1].trim();

                match key {
                    "board_size" => { /* Ignorer, nous avons déjà le board */ }
                    "board" => {
                        data.board = value.split_whitespace().map(|s| s.to_string()).collect();
                    }
                    "pot_oop" => data.pot_oop = value.parse().unwrap_or(0.0),
                    "pot_ip" => data.pot_ip = value.parse().unwrap_or(0.0),
                    "current_player" => data.current_player = value.parse().unwrap_or(0),
                    "hands_count" => {
                        range_size = value.parse().unwrap_or(0);
                        if current_player == 0 {
                            data.oop_hands.clear();
                            data.oop_cards.clear();
                            data.oop_weights.clear();
                            data.oop_equity.clear();
                            data.oop_ev.clear();
                        } else {
                            data.ip_hands.clear();
                            data.ip_cards.clear();
                            data.ip_weights.clear();
                            data.ip_equity.clear();
                            data.ip_ev.clear();
                        }
                    }
                    "actions" => {
                        data.actions = value.split(',').map(|s| s.trim().to_string()).collect();
                    }
                    "num_actions" => { /* Ignorer, nous avons le nombre d'actions */ }
                    "strategy_data" => reading_strategy = true,
                    "action_ev_data" => {
                        reading_strategy = false;
                        reading_action_ev = true;
                    }
                    _ => {}
                }
                continue;
            }
        }

        // Traiter les données CSV
        if line.contains(',') {
            // Si nous lisons des données de stratégie
            if reading_strategy {
                let values: Vec<f64> = line
                    .split(',')
                    .map(|s| s.trim().parse().unwrap_or(0.0))
                    .collect();
                data.strategy.extend(values);
                continue;
            }

            // Si nous lisons des données d'EV des actions
            if reading_action_ev {
                let values: Vec<f64> = line
                    .split(',')
                    .map(|s| s.trim().parse().unwrap_or(0.0))
                    .collect();
                data.action_ev.extend(values);
                continue;
            }

            // Sinon, nous lisons des données de main
            if !line.starts_with("hand,") {
                // Ignorer l'en-tête
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() >= 6 {
                    // hand,card1,card2,weight,equity,ev
                    let hand = parts[0].trim().to_string();
                    let card1 = parts[1].trim().parse().unwrap_or(0);
                    let card2 = parts[2].trim().parse().unwrap_or(0);
                    let weight = parts[3].trim().parse().unwrap_or(0.0);
                    let equity = parts[4].trim().parse().unwrap_or(0.0);
                    let ev = parts[5].trim().parse().unwrap_or(0.0);

                    if current_player == 0 {
                        data.oop_hands.push(hand);
                        data.oop_cards.push((card1, card2));
                        data.oop_weights.push(weight);
                        data.oop_equity.push(equity);
                        data.oop_ev.push(ev);
                    } else {
                        data.ip_hands.push(hand);
                        data.ip_cards.push((card1, card2));
                        data.ip_weights.push(weight);
                        data.ip_equity.push(equity);
                        data.ip_ev.push(ev);
                    }
                }
            }
        }
    }

    Ok(data)
}
