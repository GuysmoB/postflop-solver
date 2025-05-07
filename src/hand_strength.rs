use std::collections::HashSet;

use crate::Card;

// Fonction pour déterminer si un board a une full house
pub fn has_full_house(board: &[Card], len: usize) -> bool {
    let mut rank_counts = [0u8; 13];

    for i in 0..len {
        let rank = (board[i] % 13) as usize;
        rank_counts[rank] += 1;
    }

    let has_trips = rank_counts.iter().any(|&count| count >= 3);
    let pairs_count = rank_counts.iter().filter(|&&count| count >= 2).count();

    // Full house = un brelan et au moins une paire (qui peut être différente)
    has_trips && pairs_count >= 2
}

// Fonction pour déterminer si un board a deux paires
pub fn has_two_pair(board: &[Card], len: usize) -> bool {
    let mut rank_counts = [0u8; 13];

    for i in 0..len {
        let rank = (board[i] % 13) as usize;
        rank_counts[rank] += 1;
    }

    let pairs_count = rank_counts.iter().filter(|&&count| count == 2).count();
    pairs_count >= 2
}

// Fonction pour déterminer si un board a une paire
pub fn has_pair(board: &[Card], len: usize) -> bool {
    let mut rank_counts = [0u8; 13];

    for i in 0..len {
        let rank = (board[i] % 13) as usize;
        rank_counts[rank] += 1;
    }

    rank_counts.iter().any(|&count| count == 2)
}

// Fonction pour déterminer si un board a un brelan (three of a kind)
pub fn has_trips(board: &[Card], len: usize) -> bool {
    let mut rank_counts = [0u8; 13];

    for i in 0..len {
        let rank = (board[i] % 13) as usize;
        rank_counts[rank] += 1;
    }

    rank_counts.iter().any(|&count| count == 3)
}

// Fonction pour déterminer si un board a un carré (four of a kind)
pub fn has_quads(board: &[Card], len: usize) -> bool {
    let mut rank_counts = [0u8; 13];

    for i in 0..len {
        let rank = (board[i] % 13) as usize;
        rank_counts[rank] += 1;
    }

    rank_counts.iter().any(|&count| count == 4)
}

// Fonction pour compter les cartes par couleur
pub fn count_suits(board: &[Card], len: usize) -> [u8; 4] {
    let mut suit_counts = [0u8; 4];

    for i in 0..len {
        let suit = (board[i] / 13) as usize;
        suit_counts[suit] += 1;
    }

    suit_counts
}

// Fonction pour déterminer si le board a une flush
pub fn has_flush(board: &[Card], len: usize) -> bool {
    let suit_counts = count_suits(board, len);
    suit_counts.iter().any(|&count| count >= 3) // 3 cartes de même couleur = flush
}

// Fonction pour déterminer si le board a un tirage couleur
pub fn has_flush_draw(board: &[Card], len: usize) -> bool {
    let suit_counts = count_suits(board, len);
    suit_counts.iter().any(|&count| count == 2) // 2 cartes de même couleur = flush draw
}

// Fonction pour vérifier si le board a un tirage quinte
pub fn has_straight_draw(board: &[Card], len: usize) -> bool {
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

// Fonction pour vérifier si le board a une quinte complète
pub fn has_straight(board: &[Card], len: usize) -> bool {
    if len < 5 {
        return false; // Impossible d'avoir une quinte avec moins de 5 cartes
    }

    // Extraire et trier les rangs
    let mut ranks = Vec::with_capacity(len);
    for i in 0..len {
        ranks.push(board[i] % 13);
    }

    // Convertir en ensemble pour éliminer les doublons
    let unique_ranks: HashSet<u8> = ranks.into_iter().collect();
    let ranks_vec: Vec<u8> = unique_ranks.into_iter().collect();

    // Vérifier les fenêtres de 5 rangs consécutifs
    for window_start in 0..9 {
        let mut consecutive_count = 0;
        for r in window_start..window_start + 5 {
            if ranks_vec.contains(&r) {
                consecutive_count += 1;
            }
        }
        if consecutive_count >= 5 {
            return true;
        }
    }

    // Cas spécial: quinte A-5 (A,2,3,4,5)
    if ranks_vec.contains(&0) && // 2
       ranks_vec.contains(&1) && // 3
       ranks_vec.contains(&2) && // 4
       ranks_vec.contains(&3) && // 5
       ranks_vec.contains(&12)
    // A
    {
        return true;
    }

    false
}

pub fn is_highest_card(card: Card, board: &[Card], len: usize) -> bool {
    let card_rank = card % 13;

    for i in 0..len {
        let rank = board[i] % 13;
        if rank > card_rank {
            return false;
        }
    }

    true
}
