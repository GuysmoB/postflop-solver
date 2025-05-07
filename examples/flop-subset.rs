use std::collections::HashMap;
const LOW_CARDS: [u8; 4] = [0, 1, 2, 3]; // 2, 3, 4, 5
const MID_CARDS: [u8; 5] = [4, 5, 6, 7, 8]; // 6, 7, 8, 9, T
type Card = u8;

fn main() {
    println!("Board Subset Generator - Flop Analysis");

    // Charger les flops depuis le fichier
    let subset_flops = generate_subset_flops();
    println!("Loaded {} representative flops", subset_flops.len());

    // Analyser chaque flop
    for (_i, flop) in subset_flops.iter().enumerate() {
        let flop_key = generate_flop_key(flop);
        println!("{} - Key: {}", cards_to_string(flop, 3), flop_key);
    }

    // Regrouper les flops par clé
    let mut flop_clusters: HashMap<String, Vec<[Card; 3]>> = HashMap::new();
    for &flop in &subset_flops {
        let key = generate_flop_key(&flop);
        flop_clusters.entry(key).or_insert(Vec::new()).push(flop);
    }

    // Statistiques sur les clusters de flops
    println!("\n=== Flop Cluster Statistics ===");
    println!("Total flop clusters: {}", flop_clusters.len());

    // Afficher les clusters les plus grands
    let mut cluster_vec: Vec<(String, Vec<[Card; 3]>)> = flop_clusters.into_iter().collect();
    cluster_vec.sort_by(|a, b| b.1.len().cmp(&a.1.len()));

    println!("Top 10 largest flop clusters:");
    for (i, (key, flops)) in cluster_vec.iter().take(10).enumerate() {
        println!("  {}. {} flops - Key: {}", i + 1, flops.len(), key);
        println!("     Example: {}", cards_to_string(&flops[0], 3));
    }

    // Sélectionner un flop représentatif par cluster
    let mut representative_flops = Vec::new();
    for (_, flops) in &cluster_vec {
        if let Some(&flop) = flops.first() {
            representative_flops.push(flop);
        }
    }

    println!(
        "\nSelected {} representative flops (one per key)",
        representative_flops.len()
    );

    // Afficher les flops représentatifs
    println!("\n=== Sample of representative flops ===");
    for (i, flop) in representative_flops.iter().take(20).enumerate() {
        let key = generate_flop_key(flop);
        println!("{}. {} - Key: {}", i + 1, cards_to_string(flop, 3), key);
    }

    save_flops_to_file(&representative_flops, "representative_flops.txt");
}

fn generate_subset_flops() -> Vec<[Card; 3]> {
    // Charger les flops depuis le fichier externe
    let mut unique_flops = Vec::new();

    match std::fs::read_to_string("../iso_flops.txt") {
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
            println!("Error loading iso_flops.txt: {}. Using hardcoded flops.", e);
            // Flops de secours au cas où le fichier n'est pas trouvé
        }
    }

    unique_flops
}

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

    // Vérifier les paires/brelan
    if has_trips(board, 3) {
        key_parts.push("Set".to_string());
    } else if has_pair(board, 3) {
        key_parts.push("P".to_string());
    }

    // Vérifier flush/flush draw
    if has_flush(board, 3) {
        key_parts.push("F".to_string());
    } else if has_flush_draw(board, 3) {
        key_parts.push("FD".to_string());
    }

    // Vérifier straight draw
    if has_straight_draw(board, 3) {
        key_parts.push("SD".to_string());
    }

    format!("F:{}", key_parts.join("-"))
}

// Fonctions utilitaires pour l'analyse des boards
fn has_pair(board: &[Card], len: usize) -> bool {
    let mut rank_counts = [0u8; 13];

    for i in 0..len {
        let rank = (board[i] % 13) as usize;
        rank_counts[rank] += 1;
    }

    rank_counts.iter().any(|&count| count == 2)
}

fn has_trips(board: &[Card], len: usize) -> bool {
    let mut rank_counts = [0u8; 13];

    for i in 0..len {
        let rank = (board[i] % 13) as usize;
        rank_counts[rank] += 1;
    }

    rank_counts.iter().any(|&count| count == 3)
}

fn count_suits(board: &[Card], len: usize) -> [u8; 4] {
    let mut suit_counts = [0u8; 4];

    for i in 0..len {
        let suit = (board[i] / 13) as usize;
        suit_counts[suit] += 1;
    }

    suit_counts
}

fn has_flush(board: &[Card], len: usize) -> bool {
    let suit_counts = count_suits(board, len);
    suit_counts.iter().any(|&count| count >= 3)
}

fn has_flush_draw(board: &[Card], len: usize) -> bool {
    let suit_counts = count_suits(board, len);
    suit_counts.iter().any(|&count| count == 2)
}

fn has_straight_draw(board: &[Card], len: usize) -> bool {
    if len < 2 {
        return false;
    }

    let mut ranks = Vec::with_capacity(len);
    for i in 0..len {
        ranks.push(board[i] % 13);
    }
    ranks.sort_unstable();

    // Éliminer les doublons
    let mut unique_ranks: Vec<u8> = Vec::new();
    for &rank in &ranks {
        if !unique_ranks.contains(&rank) {
            unique_ranks.push(rank);
        }
    }

    // Vérifier les écarts entre rangs consécutifs
    let mut connected_count = 0;
    for i in 1..unique_ranks.len() {
        if unique_ranks[i] == unique_ranks[i - 1] + 1 || unique_ranks[i] == unique_ranks[i - 1] + 2
        {
            connected_count += 1;
        }
    }

    // Cas spécial: As-2
    if unique_ranks.contains(&0) && unique_ranks.contains(&12) {
        connected_count += 1;
    }

    connected_count >= 2 // Au moins 2 cartes connectées pour un tirage
}

// Fonctions utilitaires pour la conversion et l'affichage
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

fn save_flops_to_file(flops: &[[Card; 3]], filename: &str) {
    use std::fs::File;
    use std::io::Write;

    let mut file = File::create(filename).expect("Could not create file");
    for flop in flops {
        let compact_format = cards_to_compact_string(flop, 3);
        writeln!(file, "{}", compact_format).expect("Could not write to file");
    }

    println!(
        "Representative flops saved to {} in compact format",
        filename
    );
}

// Nouvelle fonction pour convertir les cartes en format compact
fn cards_to_compact_string(cards: &[Card], length: usize) -> String {
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
    }

    result
}
