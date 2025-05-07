use std::collections::{HashMap, HashSet};

use postflop_solver::{
    card_from_string, has_flush, has_flush_draw, has_full_house, has_pair, has_quads, has_straight,
    has_straight_draw, has_trips, has_two_pair, is_highest_card,
};

// Types de cartes
type Card = u8;
type Board = [Card; 5];

// Structures pour les informations du board
#[derive(Debug, Clone)]
struct BoardFeatures {
    paired_board: bool,
    three_of_kind: bool,
    four_of_kind: bool,
    flush_draw: bool,
    completed_flush: bool,
    open_straight_draw: bool,
    gutshot_draw: bool,
    completed_straight: bool,
    high_card_count: u8,
    medium_card_count: u8,
    low_card_count: u8,
    card_gaps: u8,
    board_texture: BoardTexture,
}

const LOW_CARDS: [u8; 4] = [0, 1, 2, 3]; // 2, 3, 4, 5
const MID_CARDS: [u8; 5] = [4, 5, 6, 7, 8]; // 6, 7, 8, 9, T
const HIGH_CARDS: [u8; 4] = [9, 10, 11, 12]; // J, Q, K, A

#[derive(Debug, Clone, PartialEq, Eq)]
enum BoardTexture {
    Dry,     // Few draws, disconnected
    SemiWet, // Some drawing potential
    Wet,     // Many draws, connected
    Dynamic, // Changing significantly with future cards
    Static,  // Likely to remain similar with future cards
}

fn main() {
    println!("Board Subset Generator - With River Analysis");

    // Utiliser la liste de flops spécifiée
    let subset_flops = generate_subset_flops();
    println!("Analyzing {} representative flops", subset_flops.len());

    // Limiter le nombre de flops pour l'analyse complète si nécessaire
    let flops_to_analyze = subset_flops.len();

    let mut all_river_boards = Vec::new();
    let mut total_river_texture_counts: HashMap<&'static str, u32> = HashMap::new();

    // Analyse des flops, turns et rivers
    for flop_idx in 0..flops_to_analyze {
        let flop = subset_flops[flop_idx];
        let turn_boards = generate_all_turns(flop);

        // Limiter le nombre de turns par flop pour une analyse plus rapide
        let turns_to_analyze = turn_boards.len();

        for turn_idx in 0..turns_to_analyze {
            let turn_board = turn_boards[turn_idx];
            let river_boards = generate_all_rivers(turn_board);

            println!(
                "\nAnalyzing turn+river for board: {} + {}",
                cards_to_string(&[turn_board[0], turn_board[1], turn_board[2]], 3),
                cards_to_string(&[turn_board[3]], 1)
            );

            // Analyser les rivers et leurs clés
            let mut river_key_counts: HashMap<String, u32> = HashMap::new();
            let mut river_texture_counts: HashMap<&'static str, u32> = HashMap::new();

            for river_board in river_boards.iter() {
                let river_key = generate_river_key(river_board);
                *river_key_counts.entry(river_key.clone()).or_insert(0) += 1;

                // Compter les textures
                let features = analyze_board_features(river_board, 5);
                let texture_name = match features.board_texture {
                    BoardTexture::Dry => "Dry",
                    BoardTexture::SemiWet => "SemiWet",
                    BoardTexture::Wet => "Wet",
                    BoardTexture::Dynamic => "Dynamic",
                    BoardTexture::Static => "Static",
                };
                *river_texture_counts.entry(texture_name).or_insert(0) += 1;
                *total_river_texture_counts.entry(texture_name).or_insert(0) += 1;

                // Afficher quelques exemples
                if *river_key_counts.get(&river_key).unwrap() <= 1 {
                    println!(
                        "  River {} + {} + {} - Key: {}",
                        cards_to_string(&[river_board[0], river_board[1], river_board[2]], 3), // flop
                        cards_to_string(&[river_board[3]], 1), // carte turn
                        cards_to_string(&[river_board[4]], 1), // carte river
                        river_key
                    );
                }
            }

            // Statistiques pour ce board turn+river
            println!(
                "\n=== River key statistics for board {} + {} ===",
                cards_to_string(&[turn_board[0], turn_board[1], turn_board[2]], 3),
                cards_to_string(&[turn_board[3]], 1)
            );
            println!("Total unique river keys: {}", river_key_counts.len());

            // Afficher les clés les plus fréquentes
            let mut key_vec: Vec<(String, u32)> = river_key_counts.into_iter().collect();
            key_vec.sort_by(|a, b| b.1.cmp(&a.1));

            println!("Top 5 most common river keys:");
            for (i, (key, count)) in key_vec.iter().take(5).enumerate() {
                println!("  {}. {} occurrences - Key: {}", i + 1, count, key);
            }

            // Étendre all_river_boards avec les rivers générés
            all_river_boards.extend(river_boards);
        }
    }

    // Regrouper les boards de river par leurs clés
    let mut river_clusters: HashMap<String, Vec<[Card; 5]>> = HashMap::new();

    for &board in &all_river_boards {
        let key = generate_river_key(&board);
        river_clusters.entry(key).or_insert(Vec::new()).push(board);
    }

    println!("\n=== Overall River Statistics ===");
    println!("Total river boards analyzed: {}", all_river_boards.len());
    println!(
        "Total unique river descriptive keys: {}",
        river_clusters.len()
    );

    // Afficher la distribution globale des textures pour les rivers
    println!("\nOverall river texture distribution:");
    let total_boards = all_river_boards.len() as f32;
    for (texture, count) in total_river_texture_counts.iter() {
        let percentage = (*count as f32 / total_boards) * 100.0;
        println!("  {}: {} boards ({:.1}%)", texture, count, percentage);
    }

    // Afficher quelques exemples de chaque cluster
    let mut river_key_vec: Vec<(String, Vec<[Card; 5]>)> = river_clusters.into_iter().collect();
    river_key_vec.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    println!("\nTop 10 largest river key clusters:");
    for (i, (key, boards)) in river_key_vec.iter().take(10).enumerate() {
        println!("  {}. {} boards - Key: {}", i + 1, boards.len(), key);
        println!("     Example: {}", cards_to_string(&boards[0], 5));
    }

    // Sélectionner un représentant par cluster de clé
    let mut river_representatives = Vec::new();
    for (_, boards) in &river_key_vec {
        if let Some(&board) = boards.first() {
            river_representatives.push(board);
        }
    }

    println!(
        "\nSelected {} representative river boards (one per key)",
        river_representatives.len()
    );

    // Afficher les boards représentatifs
    println!("\n=== Representative river boards ===");
    for (i, board) in river_representatives.iter().take(20).enumerate() {
        let key = generate_river_key(board);
        println!("{}. {} - Key: {}", i + 1, cards_to_string(board, 5), key);
    }

    // Enregistrer les représentants dans un fichier
    save_boards_to_file(&river_representatives, "representative_river_boards.txt");
}

fn generate_subset_flops() -> Vec<[Card; 3]> {
    // Charger les flops depuis le fichier externe
    let mut unique_flops = Vec::new();

    match std::fs::read_to_string("representative_flops.txt") {
        Ok(contents) => {
            for line in contents.lines() {
                // Ignorer les lignes vides ou commentées
                let line = line.trim();
                if line.is_empty() || line.starts_with("//") {
                    continue;
                }

                // Extraire les 3 cartes du format "4c3c2c"
                if line.len() >= 6 {
                    let card1 = parse_card_compact(&line[0..2]);
                    let card2 = parse_card_compact(&line[2..4]);
                    let card3 = parse_card_compact(&line[4..6]);
                    unique_flops.push([card1, card2, card3]);
                }
            }
            println!(
                "Loaded {} unique flops from iso_flops.txt",
                unique_flops.len()
            );
        }
        Err(e) => {
            println!(
                "Error loading iso_flops.txt: {}. Using generated flops instead.",
                e
            );
        }
    }

    unique_flops
}

fn generate_all_turns(flop: [Card; 3]) -> Vec<[Card; 4]> {
    let mut turn_boards = Vec::new();
    let used_cards: HashSet<Card> = flop.iter().cloned().collect();

    for turn in 0..52 {
        if !used_cards.contains(&turn) {
            let mut board = [0; 4];
            board[0] = flop[0];
            board[1] = flop[1];
            board[2] = flop[2];
            board[3] = turn;
            turn_boards.push(board);
        }
    }

    turn_boards
}

fn generate_all_rivers(turn_board: [Card; 4]) -> Vec<[Card; 5]> {
    let mut river_boards = Vec::new();
    let used_cards: HashSet<Card> = turn_board.iter().cloned().collect();

    for river in 0..52 {
        if !used_cards.contains(&river) {
            let mut board = [0; 5];
            board[0] = turn_board[0];
            board[1] = turn_board[1];
            board[2] = turn_board[2];
            board[3] = turn_board[3];
            board[4] = river;
            river_boards.push(board);
        }
    }

    river_boards
}

fn analyze_board_features(board: &[Card], board_length: usize) -> BoardFeatures {
    // Extraire les rangs et couleurs
    let mut ranks = Vec::with_capacity(board_length);
    let mut suits = Vec::with_capacity(board_length);

    for i in 0..board_length {
        ranks.push(board[i] % 13);
        suits.push(board[i] / 13);
    }

    // Trier les rangs pour faciliter l'analyse
    ranks.sort_unstable();

    // Vérifier les paires, triplets, etc.
    let mut rank_counts: HashMap<u8, u8> = HashMap::new();
    for &rank in &ranks {
        *rank_counts.entry(rank).or_insert(0) += 1;
    }

    let paired_board = rank_counts.values().any(|&count| count == 2);
    let three_of_kind = rank_counts.values().any(|&count| count == 3);
    let four_of_kind = rank_counts.values().any(|&count| count == 4);

    // Vérifier couleur
    let mut suit_counts: HashMap<u8, u8> = HashMap::new();
    for &suit in &suits {
        *suit_counts.entry(suit).or_insert(0) += 1;
    }

    let flush_draw = suit_counts.values().any(|&count| count == 4);
    let completed_flush = suit_counts.values().any(|&count| count >= 5);

    // Vérifier quinte
    let mut straight_potential = 0;
    let unique_ranks: Vec<u8> = rank_counts.keys().cloned().collect();
    for i in 1..unique_ranks.len() {
        if unique_ranks[i] == unique_ranks[i - 1] + 1 {
            straight_potential += 1;
        }
    }

    let open_straight_draw = straight_potential >= 3;
    let gutshot_draw = straight_potential >= 2;
    let completed_straight = is_straight(&ranks);

    // Compter les cartes par catégories
    let high_card_count = ranks.iter().filter(|&&r| r >= 9).count() as u8; // J ou plus
    let medium_card_count = ranks.iter().filter(|&&r| r >= 5 && r <= 8).count() as u8; // 7-T
    let low_card_count = ranks.iter().filter(|&&r| r < 5).count() as u8; // 2-6

    // Calculer les écarts entre cartes
    let mut card_gaps = 0;
    for i in 1..ranks.len() {
        card_gaps += ranks[i] - ranks[i - 1];
    }

    // Déterminer la texture du board
    let board_texture = determine_board_texture(
        paired_board,
        three_of_kind,
        completed_flush,
        flush_draw,
        completed_straight,
        open_straight_draw,
        card_gaps,
    );

    BoardFeatures {
        paired_board,
        three_of_kind,
        four_of_kind,
        flush_draw,
        completed_flush,
        open_straight_draw,
        gutshot_draw,
        completed_straight,
        high_card_count,
        medium_card_count,
        low_card_count,
        card_gaps,
        board_texture,
    }
}

fn is_straight(ranks: &[u8]) -> bool {
    // Cette fonction est simplifiée et peut manquer certaines quintes
    if ranks.len() < 5 {
        return false;
    }

    let unique_ranks: HashSet<u8> = ranks.iter().cloned().collect();
    let unique_ranks_vec: Vec<u8> = unique_ranks.into_iter().collect();

    // Vérifier une fenêtre de 5 rangs consécutifs
    for window_start in 0..9 {
        let mut consecutive_count = 0;
        for r in window_start..window_start + 5 {
            if unique_ranks_vec.contains(&r) {
                consecutive_count += 1;
            }
        }
        if consecutive_count >= 5 {
            return true;
        }
    }

    // Cas spécial: A-5 straight
    if unique_ranks_vec.contains(&0) && // 2
       unique_ranks_vec.contains(&1) && // 3
       unique_ranks_vec.contains(&2) && // 4
       unique_ranks_vec.contains(&3) && // 5
       unique_ranks_vec.contains(&12)
    {
        // A
        return true;
    }

    false
}

fn determine_board_texture(
    paired: bool,
    trips: bool,
    flush: bool,
    flush_draw: bool,
    straight: bool,
    straight_draw: bool,
    card_gaps: u8,
) -> BoardTexture {
    if flush || straight || trips {
        return BoardTexture::Static; // Board défini
    }

    if paired && (flush_draw || straight_draw) {
        return BoardTexture::Dynamic; // Potentiel de changement important
    }

    if flush_draw && straight_draw {
        return BoardTexture::Wet; // Beaucoup de tirages
    }

    if flush_draw || straight_draw {
        return BoardTexture::SemiWet; // Quelques tirages
    }

    if card_gaps > 10 || paired {
        return BoardTexture::Dry; // Déconnecté, peu de tirages
    }

    BoardTexture::SemiWet
}

fn save_boards_to_file(boards: &[[Card; 5]], filename: &str) {
    use std::fs::File;
    use std::io::Write;

    let mut file = File::create(filename).expect("Could not create file");

    for board in boards {
        let board_str = cards_to_string(board, 5);
        writeln!(file, "{}", board_str).expect("Could not write to file");
    }

    println!("Boards saved to {}", filename);
}

fn cards_to_string(cards: &[Card], length: usize) -> String {
    let ranks = [
        "2", "3", "4", "5", "6", "7", "8", "9", "T", "J", "Q", "K", "A",
    ];
    let suits = ["s", "h", "c", "d"];

    let mut result = String::new();

    for i in 0..length {
        let card = cards[i];
        let rank = card % 13;
        let suit = card / 13;

        result.push_str(ranks[rank as usize]);
        result.push_str(suits[suit as usize]);

        if i < length - 1 {
            result.push(' ');
        }
    }

    result
}

// Détermine la catégorie d'une carte (Low/Mid/High)
fn get_card_category(rank: u8) -> &'static str {
    if LOW_CARDS.contains(&rank) {
        "Low"
    } else if MID_CARDS.contains(&rank) {
        "Mid"
    } else {
        "High"
    }
}

fn generate_flop_key(board: &[Card; 3]) -> String {
    // Trier les cartes par rang (de façon décroissante)
    let mut sorted_board = *board;
    sorted_board.sort_by(|a, b| (b % 13).cmp(&(a % 13)));

    let mut key_parts = Vec::new();

    // Ajouter les catégories des cartes
    for i in 0..3 {
        let rank = sorted_board[i] % 13;
        key_parts.push(get_card_category(rank).to_string());
    }

    // Utiliser les fonctions utilitaires
    if has_trips(board, 3) {
        key_parts.push("Set".to_string());
    } else if has_pair(board, 3) {
        key_parts.push("P".to_string());
    }

    if has_flush(board, 3) {
        key_parts.push("F".to_string());
    } else if has_flush_draw(board, 3) {
        key_parts.push("FD".to_string());
    }

    if has_straight_draw(board, 3) {
        key_parts.push("SD".to_string());
    }

    format!("F:{}", key_parts.join("-"))
}

fn generate_turn_key(board: &[Card; 4]) -> String {
    // Obtenir la clé du flop
    let flop = [board[0], board[1], board[2]];
    let flop_key = generate_flop_key(&flop);

    // Analyser la carte turn
    let turn_card = board[3];
    let turn_rank = turn_card % 13;
    let mut turn_key_parts = Vec::new();

    // Catégorie de la turn card
    turn_key_parts.push(get_card_category(turn_rank).to_string());

    if is_highest_card(turn_card, board, 4) {
        turn_key_parts.push("Top".to_string());
    }

    if has_quads(board, 4) {
        turn_key_parts.push("Quads".to_string());
    } else if has_trips(board, 4) && !has_trips(&flop, 3) {
        turn_key_parts.push("Set".to_string());
    } else if has_pair(board, 4) && !has_pair(&flop, 3) {
        turn_key_parts.push("P".to_string());
    }

    // Vérifier flush/flush draw
    if has_flush(board, 4) && !has_flush(&flop, 3) {
        turn_key_parts.push("F".to_string());
    } else if has_flush_draw(board, 4) && !has_flush_draw(&flop, 3) {
        turn_key_parts.push("FD".to_string());
    }

    // Vérifier straight/straight draw
    if has_straight(board, 4) && !has_straight(&flop, 3) {
        turn_key_parts.push("S".to_string());
    } else if has_straight_draw(board, 4) && !has_straight_draw(&flop, 3) {
        turn_key_parts.push("SD".to_string());
    }

    // Assembler la clé complète
    format!("{} T:{}", flop_key, turn_key_parts.join("-"))
}

fn generate_river_key(board: &[Card; 5]) -> String {
    // Obtenir la clé du turn
    let turn_board = [board[0], board[1], board[2], board[3]];
    let turn_key = generate_turn_key(&turn_board);

    // Analyser la carte river
    let river_card = board[4];
    let river_rank = river_card % 13;
    let mut river_key_parts = Vec::new();

    // Catégorie de la river card
    river_key_parts.push(get_card_category(river_rank).to_string());

    // Vérifier si la river est la carte la plus haute
    if is_highest_card(river_card, board, 5) {
        river_key_parts.push("Top".to_string());
    }

    // Flop et turn pour la comparaison
    let flop = [board[0], board[1], board[2]];
    let turn = board[3];

    // Vérifier les nouvelles combinaisons apparues avec la river
    if has_quads(board, 5) && !has_quads(&turn_board, 4) {
        river_key_parts.push("Quads".to_string());
    } else if has_full_house(board, 5) && !has_full_house(&turn_board, 4) {
        river_key_parts.push("FH".to_string());
    } else if has_trips(board, 5) && !has_trips(&turn_board, 4) {
        river_key_parts.push("Set".to_string());
    } else if has_two_pair(board, 5) && !has_two_pair(&turn_board, 4) {
        river_key_parts.push("2P".to_string());
    } else if has_pair(board, 5) && !has_pair(&turn_board, 4) {
        river_key_parts.push("P".to_string());
    }

    // Vérifier flush
    if has_flush(board, 5) && !has_flush(&turn_board, 4) {
        river_key_parts.push("F".to_string());
    }

    // Vérifier straight
    if has_straight(board, 5) && !has_straight(&turn_board, 4) {
        river_key_parts.push("S".to_string());
    }

    // Assembler la clé complète
    format!("{} R:{}", turn_key, river_key_parts.join("-"))
}

fn parse_card_compact(card_str: &str) -> Card {
    let mut chars = card_str.chars();
    let rank_char = chars.next().unwrap();
    let suit_char = chars.next().unwrap();

    let rank = match rank_char {
        '2' => 0,
        '3' => 1,
        '4' => 2,
        '5' => 3,
        '6' => 4,
        '7' => 5,
        '8' => 6,
        '9' => 7,
        'T' => 8,
        'J' => 9,
        'Q' => 10,
        'K' => 11,
        'A' => 12,
        _ => panic!("Invalid rank: {}", rank_char),
    };

    let suit = match suit_char {
        'c' => 2,
        'd' => 3,
        'h' => 1,
        's' => 0,
        _ => panic!("Invalid suit: {}", suit_char),
    };

    rank + (suit * 13)
}
