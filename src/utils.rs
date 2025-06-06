use std::fs::File;
use std::io::BufWriter;
use std::io::Write;
use std::path::Path;

use crate::action_tree::Action;
use crate::deal;
use crate::holes_to_strings;
use crate::play;
use crate::results::select_spot;
use crate::save_exploration_results;
use crate::ActionTree;
use crate::Card;
use crate::GameState;
use crate::PostFlopGame;
use crate::Spot;
use crate::SpotType;
use rand::Rng;

/// Structure pour stocker une carte prédéfinie
#[derive(Clone, Debug)]
pub struct PredefinedCard {
    pub card_index: usize,
    pub card_value: Card,
}

#[derive(Debug)]
pub enum ActionPattern {
    Exact(Action),
    BetWildcard,
    RaiseWildcard,
    AllInWildcard,
}

#[derive(Clone)]
pub struct SpecificResultData {
    pub current_player: String,
    pub num_actions: usize,
    pub is_empty: bool,
    pub eqr_base: [i32; 2],
    pub weights: Vec<Vec<f64>>,
    pub normalizer: Vec<Vec<f64>>,
    pub equity: Vec<Vec<f64>>,
    pub ev: Vec<Vec<f64>>,
    pub eqr: Vec<Vec<f64>>,
    pub strategy: Vec<f64>,
    pub action_ev: Vec<f64>,
    pub oop_cards: Vec<(u8, u8)>,
    pub ip_cards: Vec<(u8, u8)>,
}

impl Default for SpecificResultData {
    fn default() -> Self {
        Self {
            current_player: String::new(),
            num_actions: 0,
            is_empty: true,
            eqr_base: [0, 0],
            weights: vec![Vec::new(), Vec::new()],
            normalizer: vec![Vec::new(), Vec::new()],
            equity: vec![Vec::new(), Vec::new()],
            ev: vec![Vec::new(), Vec::new()],
            eqr: vec![Vec::new(), Vec::new()],
            strategy: Vec::new(),
            action_ev: Vec::new(),
            oop_cards: vec![(0, 0); 52],
            ip_cards: vec![(0, 0); 52],
        }
    }
}

#[derive(Clone)]
pub struct SpecificChanceReportData {
    pub current_player: String,
    pub num_actions: usize,
    pub status: Vec<f64>,
    pub combos: Vec<Vec<f64>>,
    pub equity: Vec<Vec<f64>>,
    pub ev: Vec<Vec<f64>>,
    pub eqr: Vec<Vec<f64>>,
    pub strategy: Vec<f64>,
}

impl Default for SpecificChanceReportData {
    fn default() -> Self {
        Self {
            current_player: String::new(),
            num_actions: 0,
            status: vec![0.0; 52],
            combos: vec![vec![0.0; 52], vec![0.0; 52]],
            equity: vec![vec![0.0; 52], vec![0.0; 52]],
            ev: vec![vec![0.0; 52], vec![0.0; 52]],
            eqr: vec![vec![0.0; 52], vec![0.0; 52]],
            strategy: Vec::new(),
        }
    }
}

// Fonction utilitaire pour convertir une carte en chaîne simple
pub fn card_to_string_simple(card: Card) -> String {
    let rank_chars = [
        '2', '3', '4', '5', '6', '7', '8', '9', 'T', 'J', 'Q', 'K', 'A',
    ];
    let suit_chars = ['c', 'd', 'h', 's'];

    let rank = (card >> 2) as usize;
    let suit = (card & 3) as usize;

    if rank < rank_chars.len() && suit < suit_chars.len() {
        format!("{}{}", rank_chars[rank], suit_chars[suit])
    } else {
        "??".to_string()
    }
}

#[inline]
pub fn round(value: f64) -> f64 {
    if value < 1.0 {
        (value * 1000000.0).round() / 1000000.0
    } else if value < 10.0 {
        (value * 100000.0).round() / 100000.0
    } else if value < 100.0 {
        (value * 10000.0).round() / 10000.0
    } else if value < 1000.0 {
        (value * 1000.0).round() / 1000.0
    } else if value < 10000.0 {
        (value * 100.0).round() / 100.0
    } else {
        (value * 10.0).round() / 10.0
    }
}

#[inline]
fn round_iter<'a>(iter: impl Iterator<Item = &'a f32> + 'a) -> impl Iterator<Item = f64> + 'a {
    iter.map(|&x| round(x as f64))
}

pub fn get_results(game: &mut PostFlopGame) -> Box<[f64]> {
    let mut buf = Vec::new();

    let total_bet_amount = game.total_bet_amount();
    let pot_base = (game.tree_config().starting_pot as f64)  // Convertir en f64
    + total_bet_amount
        .iter()
        .fold(0.0f64, |a, b| a.min(*b as f64));

    buf.push(pot_base + (total_bet_amount[0] as f64));
    buf.push(pot_base + (total_bet_amount[1] as f64));

    let trunc = |&w: &f32| if w < 0.0005 { 0.0 } else { w };
    let weights = [
        game.weights(0).iter().map(trunc).collect::<Vec<_>>(),
        game.weights(1).iter().map(trunc).collect::<Vec<_>>(),
    ];

    let is_empty = |player: usize| weights[player].iter().all(|&w| w == 0.0);
    let is_empty_flag = is_empty(0) as usize + 2 * is_empty(1) as usize;
    buf.push(is_empty_flag as f64);

    buf.extend(round_iter(weights[0].iter()));
    buf.extend(round_iter(weights[1].iter()));

    if is_empty_flag > 0 {
        buf.extend(round_iter(weights[0].iter()));
        buf.extend(round_iter(weights[1].iter()));
    } else {
        game.cache_normalized_weights();

        buf.extend(round_iter(game.normalized_weights(0).iter()));
        buf.extend(round_iter(game.normalized_weights(1).iter()));

        let equity = [game.equity(0), game.equity(1)];
        let ev = [game.expected_values(0), game.expected_values(1)];

        buf.extend(round_iter(equity[0].iter()));
        buf.extend(round_iter(equity[1].iter()));
        buf.extend(round_iter(ev[0].iter()));
        buf.extend(round_iter(ev[1].iter()));

        for player in 0..2 {
            let pot = pot_base + (total_bet_amount[player] as f64);
            for (&eq, &ev) in equity[player].iter().zip(ev[player].iter()) {
                let (eq, ev) = (eq as f64, ev as f64);
                if eq < 5e-7 {
                    buf.push(ev / 0.0);
                } else {
                    buf.push(round(ev / (pot * eq)));
                }
            }
        }
    }

    if !game.is_terminal_node() && !game.is_chance_node() {
        // println!("get_result() - before strategy()");
        buf.extend(round_iter(game.strategy().iter()));
        // println!("get_result() - after strategy()");

        if is_empty_flag == 0 {
            buf.extend(round_iter(
                game.expected_values_detail(game.current_player()).iter(),
            ));
        }
    }

    buf.into_boxed_slice()
}

pub fn weighted_average(slice: &[f32], weights: &[f32]) -> f64 {
    let mut sum = 0.0;
    let mut weight_sum = 0.0;
    for (&value, &weight) in slice.iter().zip(weights.iter()) {
        sum += value as f64 * weight as f64;
        weight_sum += weight as f64;
    }
    sum / weight_sum
}

pub fn get_specific_result(
    game: &mut PostFlopGame,
    current_player: &str,
    num_actions: usize,
) -> Result<SpecificResultData, String> {
    let buffer = get_results(game);

    // Save buffer to JSON file
    // let json_path = format!("{}/buffer_{}.json", "solver_results", current_player);
    // let file =
    //     File::create(&json_path).map_err(|e| format!("Failed to create JSON file: {}", e))?;
    // let mut writer = BufWriter::new(file);

    // let mut json_parts = Vec::new();
    // json_parts.push("{\n".to_string());

    // // Ajouter chaque entrée manuellement avec les indices en ordre numérique
    // for i in 0..buffer.len() {
    //     json_parts.push(format!("    \"{}\": {},\n", i, buffer[i]));
    // }

    // // Supprimer la dernière virgule et fermer l'objet
    // if json_parts.len() > 1 {
    //     let last_idx = json_parts.len() - 1;
    //     let last_entry = &json_parts[last_idx];
    //     json_parts[last_idx] = last_entry.trim_end_matches(",\n").to_string() + "\n";
    // }
    // json_parts.push("}".to_string());

    // let json_data = json_parts.join("");

    // // Écrire dans le fichier
    // writer
    //     .write_all(json_data.as_bytes())
    //     .map_err(|e| format!("Failed to write JSON data: {}", e))?;

    // writer
    //     .flush()
    //     .map_err(|e| format!("Failed to flush JSON data: {}", e))?;

    // println!(
    //     "Buffer saved to {} with proper numerical ordering",
    //     json_path
    // );

    // 2. Déterminer les tailles des ranges
    let oop_range_size = game.private_cards(0).len();
    let ip_range_size = game.private_cards(1).len();
    let length = [oop_range_size, ip_range_size];

    // Extraire les cartes privées des joueurs
    let oop_cards = game
        .private_cards(0)
        .iter()
        .map(|&c| (c.0 as u8, c.1 as u8))
        .collect::<Vec<_>>();

    let ip_cards = game
        .private_cards(1)
        .iter()
        .map(|&c| (c.0 as u8, c.1 as u8))
        .collect::<Vec<_>>();

    // 3. Parser le buffer comme dans le frontend
    let mut offset = 0;
    let mut weights: Vec<Vec<f64>> = vec![Vec::new(), Vec::new()];
    let mut normalizer: Vec<Vec<f64>> = vec![Vec::new(), Vec::new()];
    let mut equity: Vec<Vec<f64>> = vec![Vec::new(), Vec::new()];
    let mut ev: Vec<Vec<f64>> = vec![Vec::new(), Vec::new()];
    let mut eqr: Vec<Vec<f64>> = vec![Vec::new(), Vec::new()];
    let mut strategy: Vec<f64> = Vec::new();
    let mut action_ev: Vec<f64> = Vec::new();

    // Header: pot OOP, pot IP, is_empty_flag
    let eqr_base = [buffer[0] as i32, buffer[1] as i32];
    offset += 2;

    let is_empty_flag = buffer[offset] as usize;
    let is_empty = is_empty_flag == 3; // OOP et IP ranges sont toutes les deux vides
    offset += 1;

    // Weights
    for i in 0..length[0] {
        weights[0].push(buffer[offset + i]);
    }
    offset += length[0];

    for i in 0..length[1] {
        weights[1].push(buffer[offset + i]);
    }
    offset += length[1];

    if is_empty_flag > 0 {
        // Si vide, normalizer = weights
        normalizer = weights.clone();
    } else {
        // Normalizer weights
        for i in 0..length[0] {
            normalizer[0].push(buffer[offset + i]);
        }
        offset += length[0];

        for i in 0..length[1] {
            normalizer[1].push(buffer[offset + i]);
        }
        offset += length[1];

        // Equity
        for i in 0..length[0] {
            equity[0].push(buffer[offset + i]);
        }
        offset += length[0];

        for i in 0..length[1] {
            equity[1].push(buffer[offset + i]);
        }
        offset += length[1];

        // EV
        for i in 0..length[0] {
            ev[0].push(buffer[offset + i]);
        }
        offset += length[0];

        for i in 0..length[1] {
            ev[1].push(buffer[offset + i]);
        }
        offset += length[1];

        // EQR
        for i in 0..length[0] {
            eqr[0].push(buffer[offset + i]);
        }
        offset += length[0];

        for i in 0..length[1] {
            eqr[1].push(buffer[offset + i]);
        }
        offset += length[1];
    }

    // Strategy et action EV pour le joueur actuel
    if ["oop", "ip"].contains(&current_player) {
        let player_index = if current_player == "oop" { 0 } else { 1 };
        let range_size = length[player_index];

        // Strategy
        for i in 0..(num_actions * range_size) {
            if offset + i < buffer.len() {
                strategy.push(buffer[offset + i]);
            } else {
                strategy.push(0.0);
            }
        }
        offset += num_actions * range_size;

        // Action EV (si pas vide)
        if !is_empty {
            for i in 0..(num_actions * range_size) {
                if offset + i < buffer.len() {
                    action_ev.push(buffer[offset + i]);
                } else {
                    action_ev.push(0.0);
                }
            }
        }
    }

    // 4. Construire et retourner le résultat
    Ok(SpecificResultData {
        current_player: current_player.to_string(),
        num_actions,
        is_empty,
        eqr_base,
        weights,
        normalizer,
        equity,
        ev,
        eqr,
        strategy,
        action_ev,
        oop_cards,
        ip_cards,
    })
}

/// Exécuter le scénario: OOP bet, IP call, puis turn, puis OOP bet, IP call pour arriver à la river
pub fn run_bet_call_turn_scenario(game: &mut PostFlopGame) -> Result<(), String> {
    // Créer l'état du jeu
    let mut state = GameState::new();

    // Initialiser avec la racine (flop)
    let starting_pot = game.tree_config().starting_pot as f64;
    let effective_stack = game.tree_config().effective_stack as f64;
    let board = game.current_board();

    println!("\n=== DÉMARRAGE DU SCÉNARIO ===");
    println!("Pot initial: {:.2} bb", starting_pot);
    println!("Stack effectif: {:.2} bb", effective_stack);
    println!(
        "Board: {}",
        board
            .iter()
            .map(|&c| card_to_string_simple(c))
            .collect::<Vec<_>>()
            .join(" ")
    );

    // Spot racine
    let root_spot = Spot {
        spot_type: SpotType::Root,
        index: 0,
        player: "flop".to_string(),
        selected_index: -1,
        actions: Vec::new(),
        cards: Vec::new(),
        pot: starting_pot,
        stack: effective_stack,
        equity_oop: 0.0,
        prev_player: None,
    };

    state.spots.push(root_spot);

    // Sélectionner le premier spot (initialise le jeu)
    let results = select_spot(game, &mut state, 1, true, false)?;
    display_top_hands(game, 10, "ROOT", &results)?;

    // ÉTAPE 1: OOP BET SUR LE FLOP
    println!("\n=== ÉTAPE 1: OOP BET SUR LE FLOP ===");
    let bet_idx = state.spots[1]
        .actions
        .iter()
        .position(|a| a.name == "Bet")
        .ok_or_else(|| "Action Bet non trouvée pour OOP".to_string())?;

    let bet_action = &state.spots[1].actions[bet_idx];
    println!(
        "Action sélectionnée: {} {}",
        bet_action.name,
        if bet_action.amount != "0" {
            &bet_action.amount
        } else {
            ""
        }
    );

    let results = play(game, &mut state, bet_idx)?;
    display_top_hands(game, 10, "APRÈS OOP BET", &results)?;

    // ÉTAPE 2: IP CALL SUR LE FLOP
    println!("\n=== ÉTAPE 2: IP CALL SUR LE FLOP ===");
    let call_idx = state.spots[2]
        .actions
        .iter()
        .position(|a| a.name == "Call")
        .ok_or_else(|| "Action Call non trouvée pour IP".to_string())?;

    let call_action = &state.spots[2].actions[call_idx];
    println!(
        "Action sélectionnée: {} {}",
        call_action.name,
        if call_action.amount != "0" {
            &call_action.amount
        } else {
            ""
        }
    );

    let results = play(game, &mut state, call_idx)?;
    display_top_hands(game, 10, "APRÈS IP CALL", &results)?;

    // ÉTAPE 3: DISTRIBUTION DE LA TURN
    println!("\n=== ÉTAPE 3: DISTRIBUTION DE LA TURN ===");
    let chance_spot_idx = 3;
    println!("Pot actuel: {:.2} bb", state.spots[chance_spot_idx].pot);
    println!(
        "Stack restant: {:.2} bb",
        state.spots[chance_spot_idx].stack
    );

    let available_cards: Vec<usize> = state.spots[chance_spot_idx]
        .cards
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.is_dead)
        .map(|(idx, _)| idx)
        .collect();

    if available_cards.is_empty() {
        return Err("Aucune carte disponible pour la turn!".to_string());
    }

    // Sélectionner une carte aléatoire parmi les disponibles
    let mut rng = rand::thread_rng();
    let random_idx = rng.gen_range(0..available_cards.len());
    let card_idx = available_cards[random_idx];

    let selected_card = state.spots[chance_spot_idx].cards[card_idx].card as Card;
    println!(
        "Carte turn sélectionnée: {}",
        card_to_string_simple(selected_card)
    );

    deal(game, &mut state, card_idx)?;

    println!(
        "Board après la turn: {}",
        game.current_board()
            .iter()
            .map(|&c| card_to_string_simple(c))
            .collect::<Vec<_>>()
            .join(" ")
    );

    // ÉTAPE 4: OOP BET SUR LA TURN
    println!("\n=== ÉTAPE 4: OOP BET SUR LA TURN ===");
    let turn_oop_spot_idx = 4;
    let turn_bet_idx = state.spots[turn_oop_spot_idx]
        .actions
        .iter()
        .position(|a| a.name == "Bet")
        .ok_or_else(|| "Action Bet non trouvée pour OOP sur la turn".to_string())?;

    let turn_bet_action = &state.spots[turn_oop_spot_idx].actions[turn_bet_idx];
    println!(
        "Action sélectionnée: {} {}",
        turn_bet_action.name,
        if turn_bet_action.amount != "0" {
            &turn_bet_action.amount
        } else {
            ""
        }
    );

    play(game, &mut state, turn_bet_idx)?;
    display_top_hands(game, 10, "APRÈS OOP BET SUR LA TURN", &results)?;

    // ÉTAPE 5: IP CALL SUR LA TURN
    println!("\n=== ÉTAPE 5: IP CALL SUR LA TURN ===");
    let turn_ip_spot_idx = 5;
    let turn_call_idx = state.spots[turn_ip_spot_idx]
        .actions
        .iter()
        .position(|a| a.name == "Call")
        .ok_or_else(|| "Action Call non trouvée pour IP sur la turn".to_string())?;

    let turn_call_action = &state.spots[turn_ip_spot_idx].actions[turn_call_idx];
    println!(
        "Action sélectionnée: {} {}",
        turn_call_action.name,
        if turn_call_action.amount != "0" {
            &turn_call_action.amount
        } else {
            ""
        }
    );

    play(game, &mut state, turn_call_idx)?;

    // ÉTAPE 6: ARRIVÉE À LA RIVER
    println!("\n=== ÉTAPE 3: DISTRIBUTION DE LA RIVER ===");
    let chance_spot_idx = 6;
    println!("Pot actuel: {:.2} bb", state.spots[chance_spot_idx].pot);
    println!(
        "Stack restant: {:.2} bb",
        state.spots[chance_spot_idx].stack
    );

    let available_cards: Vec<usize> = state.spots[chance_spot_idx]
        .cards
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.is_dead)
        .map(|(idx, _)| idx)
        .collect();

    if available_cards.is_empty() {
        return Err("Aucune carte disponible pour la river!".to_string());
    }

    // Sélectionner une carte aléatoire parmi les disponibles
    let mut rng = rand::thread_rng();
    let random_idx = rng.gen_range(0..available_cards.len());
    let card_idx = available_cards[random_idx];

    let selected_card = state.spots[chance_spot_idx].cards[card_idx].card as Card;
    println!(
        "Carte river sélectionnée: {}",
        card_to_string_simple(selected_card)
    );

    deal(game, &mut state, card_idx)?;

    println!(
        "Board après la river: {}",
        game.current_board()
            .iter()
            .map(|&c| card_to_string_simple(c))
            .collect::<Vec<_>>()
            .join(" ")
    );

    Ok(())
}

/// Explorer l'arbre des actions de manière interactive (version simplifiée)
pub fn explore_game_tree(game: &mut PostFlopGame) -> Result<(), String> {
    let mut state = GameState::new();
    let mut history_stack: Vec<usize> = Vec::new(); // Indices de spots pour revenir en arrière
    let starting_pot = game.tree_config().starting_pot as f64;
    let effective_stack = game.tree_config().effective_stack as f64;
    let board = game.current_board();

    // Flag pour indiquer si nous venons de revenir en arrière sur un nœud chance
    let mut came_from_back = false;

    println!("=== EXPLORATEUR D'ARBRE INTERACTIF ===");
    println!(
        "Board: {}",
        board
            .iter()
            .map(|&c| card_to_string_simple(c))
            .collect::<Vec<_>>()
            .join(" ")
    );

    let root_spot = Spot {
        spot_type: SpotType::Root,
        index: 0,
        player: "flop".to_string(),
        selected_index: -1,
        actions: Vec::new(),
        cards: Vec::new(),
        pot: starting_pot,
        stack: effective_stack,
        equity_oop: 0.0,
        prev_player: None,
    };

    state.spots.push(root_spot);
    select_spot(game, &mut state, 1, true, false)?;

    loop {
        // Si nous avons une référence à un nœud chance (selected_chance_index > -1)
        // ET nous ne venons pas juste de revenir en arrière
        if state.selected_chance_index > -1 && !came_from_back {
            let chance_index = state.selected_chance_index as usize;

            println!("\n=== NŒUD CHANCE DÉTECTÉ (Index: {}) ===", chance_index);

            if let Some(chance_spot) = state.spots.get(chance_index) {
                if chance_spot.spot_type == SpotType::Chance {
                    // Sauvegarder l'état actuel
                    history_stack.push(chance_index);

                    // Collecter les cartes disponibles
                    let available_cards: Vec<usize> = chance_spot
                        .cards
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| !c.is_dead)
                        .map(|(idx, _)| idx)
                        .collect();

                    if !available_cards.is_empty() {
                        // Choisir une carte aléatoire
                        let mut rng = rand::thread_rng();
                        let random_card_idx = rng.gen_range(0..available_cards.len());
                        let card_idx = available_cards[random_card_idx];

                        // Log de la carte sélectionnée
                        let selected_card = chance_spot.cards[card_idx].card as Card;
                        println!(
                            "\n=== CARTE ALÉATOIRE DISTRIBUÉE ===\nCarte: {}",
                            card_to_string_simple(selected_card)
                        );

                        // Distribuer la carte
                        deal(game, &mut state, card_idx)?;

                        // Afficher immédiatement le board mis à jour
                        println!(
                            "Board mis à jour: {}",
                            game.current_board()
                                .iter()
                                .map(|&c| card_to_string_simple(c))
                                .collect::<Vec<_>>()
                                .join(" ")
                        );

                        // Continuer directement avec la boucle suivante
                        continue;
                    } else {
                        println!("Aucune carte disponible pour la distribution!");
                    }
                } else {
                    println!(
                        "ERREUR: Le nœud à l'index {} n'est pas un nœud chance",
                        chance_index
                    );
                }
            }
        }

        // Réinitialiser le flag après avoir traversé la vérification de chance
        came_from_back = false;

        let current_spot_index = state.selected_spot_index as usize;

        // Afficher le board actuel
        println!("\n=== ÉTAT ACTUEL ===");
        println!(
            "Board: {}",
            game.current_board()
                .iter()
                .map(|&c| card_to_string_simple(c))
                .collect::<Vec<_>>()
                .join(" ")
        );

        // Afficher les informations du spot actuel
        print_current_state(&state);

        // Options utilisateur
        println!("\nEntrez un nombre pour choisir une action");
        println!("r: Retour arrière");
        println!("q: Quitter");
        println!("h: Afficher l'historique des actions");

        if state.selected_chance_index > -1 {
            println!("c: Distribuer une carte");
            println!("s: Sauter ce nœud chance (revenir à l'état précédent)");
        }

        // Lire l'entrée utilisateur
        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .expect("Échec de la lecture");
        let input = input.trim();

        if input == "q" {
            println!("Au revoir!");
            break;
        } else if input == "r" {
            // Revenir en arrière
            if history_stack.is_empty() {
                println!("Vous êtes déjà à la racine.");
            } else {
                let spot_index = history_stack.pop().unwrap();

                // Recréer l'état à partir de la racine
                state.spots.truncate(spot_index);
                game.back_to_root();

                // Rejouer l'historique
                let mut history = Vec::new();
                for i in 1..spot_index {
                    if state.spots[i].selected_index != -1 {
                        history.push(state.spots[i].selected_index as usize);
                    }
                }
                game.apply_history(&history);

                select_spot(game, &mut state, spot_index, true, false)?;
                println!("Retour au spot précédent.");

                // Activer le flag si nous sommes revenus à un nœud chance
                came_from_back = state.selected_chance_index > -1;

                // Si nous sommes revenus à un nœud chance, afficher un message spécial
                if came_from_back {
                    println!("\n=== RETOUR À UN NŒUD CHANCE ===");
                    println!("Utilisez 'c' pour distribuer une carte ou 'r' pour revenir encore en arrière.");
                }
            }
        } else if input == "h" {
            // Afficher l'historique des actions
            // print_action_history(&state);
        } else if input == "s" && state.selected_chance_index > -1 {
            // Option pour sauter le nœud chance et revenir à l'état précédent
            if history_stack.is_empty() {
                println!("Impossible de sauter, vous êtes à la racine.");
            } else {
                let spot_index = history_stack.pop().unwrap();
                // Revenir à l'action joueur précédente
                while !history_stack.is_empty() {
                    let prev_spot_index = history_stack.pop().unwrap();
                    if let Some(spot) = state.spots.get(prev_spot_index) {
                        if spot.spot_type == SpotType::Player {
                            // Recréer l'état à partir de la racine
                            state.spots.truncate(prev_spot_index);
                            game.back_to_root();

                            // Rejouer l'historique
                            let mut history = Vec::new();
                            for i in 1..prev_spot_index {
                                if state.spots[i].selected_index != -1 {
                                    history.push(state.spots[i].selected_index as usize);
                                }
                            }
                            game.apply_history(&history);

                            select_spot(game, &mut state, prev_spot_index, true, false)?;
                            println!("Saut du nœud chance - retour au joueur précédent.");
                            break;
                        }
                    }
                }
            }
        } else if input == "c" && state.selected_chance_index > -1 {
            // L'utilisateur veut explicitement distribuer une carte - on laisse
            // le nœud chance être traité au début de la prochaine itération
            continue;
        } else if let Ok(index) = input.parse::<usize>() {
            if let Some(spot) = state.spots.get(current_spot_index) {
                if spot.spot_type == SpotType::Player {
                    if index < spot.actions.len() {
                        // Sauvegarder l'état actuel
                        history_stack.push(current_spot_index);

                        // Log de l'action sélectionnée
                        println!(
                            "\n=== ACTION SÉLECTIONNÉE ===\n{} choisit: {} {}",
                            spot.player.to_uppercase(),
                            spot.actions[index].name,
                            if spot.actions[index].amount != "0" {
                                &spot.actions[index].amount
                            } else {
                                ""
                            }
                        );

                        // Jouer l'action
                        play(game, &mut state, index)?;

                        // Si l'action a créé un nœud chance, le traitement se fera
                        // automatiquement au début de la prochaine itération
                    } else {
                        println!("Action invalide.");
                    }
                } else {
                    println!("Ce spot ne permet pas de choisir une action.");
                }
            }
        } else {
            println!("Commande non reconnue.");
        }
    }

    Ok(())
}

/// Fonction auxiliaire pour afficher l'état actuel du jeu
fn print_current_state(state: &GameState) {
    let current_spot_index = state.selected_spot_index as usize;

    if let Some(spot) = state.spots.get(current_spot_index) {
        println!("\n=== SPOT ACTUEL (Index: {}) ===", current_spot_index);
        println!("Type: {:?}", spot.spot_type);

        if spot.player != "flop" {
            println!("Joueur: {}", spot.player);
        }

        println!("Pot: {:.2} bb", spot.pot);
        println!("Stack restant: {:.2} bb", spot.stack);

        match spot.spot_type {
            SpotType::Player => {
                println!("\nActions disponibles:");
                for (i, action) in spot.actions.iter().enumerate() {
                    println!(
                        "  {}: {} {}",
                        i,
                        action.name,
                        if action.amount != "0" {
                            &action.amount
                        } else {
                            ""
                        }
                    );
                }
            }
            SpotType::Chance => {
                println!("\nCartes disponibles (non mortes):");
                let mut shown_count = 0;
                for (i, card) in spot.cards.iter().enumerate() {
                    if !card.is_dead {
                        println!("  {}: {}", i, card_to_string_simple(card.card as Card));

                        // Limiter l'affichage à 10 cartes pour ne pas submerger l'utilisateur
                        shown_count += 1;
                        if shown_count >= 10 {
                            let remaining = spot.cards.iter().filter(|c| !c.is_dead).count() - 10;
                            if remaining > 0 {
                                println!("  ... et {} autres cartes", remaining);
                            }
                            break;
                        }
                    }
                }
            }
            SpotType::Terminal => {
                println!("Équité OOP: {:.2}%", spot.equity_oop * 100.0);
            }
            _ => {}
        }
    }
}

pub fn actions_after(game: &mut PostFlopGame, append: &[usize]) -> String {
    if append.is_empty() {
        return get_current_actions_string(game);
    }

    let history = game.cloned_history();
    for &action in append {
        if game.is_chance_node() {
            let possible_cards = game.possible_cards();

            if possible_cards == 0 {
                return "error: no cards available".to_string();
            }

            // Si action=0, prendre la première carte disponible
            let card_to_play = if action == 0 {
                possible_cards.trailing_zeros() as usize
            } else if action < 52 && (possible_cards & (1u64 << action)) != 0 {
                action
            } else {
                return format!("error: invalid card {}", action);
            };

            game.play(card_to_play);
        } else {
            game.play(action);
        }
    }

    let result = get_current_actions_string(game);
    game.apply_history(&history);
    result
}

pub fn get_current_actions_string(game: &PostFlopGame) -> String {
    if game.is_terminal_node() {
        "terminal".to_string()
    } else if game.is_chance_node() {
        "chance".to_string()
    } else {
        game.available_actions()
            .iter()
            .map(|action| match action {
                Action::Fold => "Fold:0".to_string(),
                Action::Check => "Check:0".to_string(),
                Action::Call => "Call:0".to_string(),
                Action::Bet(amount) => format!("Bet:{}", amount),
                Action::Raise(amount) => format!("Raise:{}", amount),
                Action::AllIn(amount) => format!("Allin:{}", amount),
                _ => unreachable!(),
            })
            .collect::<Vec<_>>()
            .join("/")
    }
}

pub fn current_player_str(game: &PostFlopGame) -> &'static str {
    if game.is_terminal_node() {
        "terminal"
    } else if game.is_chance_node() {
        "chance"
    } else if game.current_player() == 0 {
        "oop"
    } else {
        "ip"
    }
}

pub fn total_bet_amount(game: &mut PostFlopGame, append: &[usize]) -> Vec<u32> {
    if append.is_empty() {
        let total_bet_amount = game.total_bet_amount();
        return total_bet_amount.iter().map(|&x| x as u32).collect();
    }
    let history = game.history().to_vec();
    for &action in append {
        game.play(action);
    }
    let total_bet_amount = game.total_bet_amount();
    let ret = total_bet_amount.iter().map(|&x| x as u32).collect();
    game.apply_history(&history);
    ret
}

/// Fonction récursive pour explorer tous les chemins possibles dans l'arbre de décision
pub fn explore_recursive(
    game: &mut PostFlopGame,
    state: &mut GameState,
    path: &mut Vec<String>,
    predefined_cards: &mut Vec<PredefinedCard>,
    depth: usize,
    max_depth: usize,
    paths_explored: &mut i32,
    terminals_reached: &mut i32,
    verbose: bool, // Paramètre pour contrôler le niveau de détail des logs
) -> Result<(), String> {
    // Éviter une profondeur excessive
    if depth >= max_depth {
        if verbose {
            println!("Profondeur maximale atteinte ({} niveaux)", max_depth);
        }
        return Ok(());
    }

    *paths_explored += 1;

    if depth == 0 {
        path.push("Flop".to_string());
    }

    // Traiter d'abord les nœuds chance
    if state.selected_chance_index > -1 {
        let chance_index = state.selected_chance_index as usize;

        if verbose {
            println!("\n=== NŒUD CHANCE DÉTECTÉ (Index: {}) ===", chance_index);
        }

        // Vérifier si l'index est valide
        if chance_index >= state.spots.len() {
            return Err(format!(
                "Index de chance invalide: {} (taille spots: {})",
                chance_index,
                state.spots.len()
            ));
        }

        let chance_spot = &state.spots[chance_index];

        // Vérifier que c'est bien un nœud chance
        if chance_spot.spot_type != SpotType::Chance {
            return Err(format!(
                "Le nœud à l'index {} n'est pas un nœud chance (type: {:?})",
                chance_index, chance_spot.spot_type
            ));
        }

        let is_turn = chance_spot.player == "turn";
        let card_type_index = if is_turn { 0 } else { 1 };

        // Sélection d'une carte
        let card_index = if card_type_index < predefined_cards.len() {
            predefined_cards[card_type_index].card_index
        } else {
            // Collecter les cartes disponibles
            let available_cards: Vec<usize> = chance_spot
                .cards
                .iter()
                .enumerate()
                .filter(|(_, c)| !c.is_dead)
                .map(|(idx, _)| idx)
                .collect();

            if available_cards.is_empty() {
                return Err("Aucune carte disponible pour la distribution".to_string());
            }

            // Sélectionner une carte aléatoire
            let mut rng = rand::thread_rng();
            let random_card_idx = rng.gen_range(0..available_cards.len());
            let idx = available_cards[random_card_idx];

            // Stocker la carte pour réutilisation
            let card_value = chance_spot.cards[idx].card as Card;
            predefined_cards.push(PredefinedCard {
                card_index: idx,
                card_value,
            });

            idx
        };

        // Log de la carte distribuée
        let card_value = chance_spot.cards[card_index].card as Card;
        let card_str = card_to_string_simple(card_value);
        path.push(format!("{}: {}", chance_spot.player, card_str));

        if verbose {
            println!("Distribution de la carte: {}", card_str);
        }

        // Sauvegarder l'état actuel
        let history_before = game.cloned_history();
        let mut new_state = state.clone();

        // Distribuer la carte
        deal(game, &mut new_state, card_index)?;

        // Continuer l'exploration avec le nouvel état
        explore_recursive(
            game,
            &mut new_state,
            path,
            predefined_cards,
            depth + 1,
            max_depth,
            paths_explored,
            terminals_reached,
            verbose,
        )?;

        // Restaurer l'état du jeu
        game.apply_history(&history_before);

        // Retirer la dernière action du chemin
        path.pop();

        return Ok(());
    }

    let current_spot_index = state.selected_spot_index as usize;
    let current_spot = match state.spots.get(current_spot_index) {
        Some(spot) => spot,
        None => return Err(format!("Spot à l'index {} non trouvé", current_spot_index)),
    };

    // Gestion selon le type de nœud
    match current_spot.spot_type {
        // 1. Nœud terminal - afficher le résultat complet et revenir
        SpotType::Terminal => {
            *terminals_reached += 1;
            return Ok(());
        }

        // 2. Nœud joueur - explorer toutes les actions possibles
        SpotType::Player => {
            // Explorer chaque action
            for (i, action) in current_spot.actions.iter().enumerate() {
                // Log de l'action sélectionnée
                let action_name = format!(
                    "{}: {} {}",
                    current_spot.player.to_uppercase(),
                    action.name,
                    if action.amount != "0" {
                        &action.amount
                    } else {
                        ""
                    }
                );

                // Afficher les taux d'action (stratégies)
                let rate_str = if action.rate >= 0.0 {
                    format!(" (taux: {:.1}%)", action.rate * 100.0)
                } else {
                    "".to_string()
                };

                // Ajouter l'action au chemin
                path.push(format!("{}{}", action_name, rate_str));

                // Sauvegarder l'état actuel
                let history_before = game.cloned_history();
                let mut new_state = state.clone();

                // Jouer l'action, mais sans afficher de détails
                play(game, &mut new_state, i)?;

                // Continuer l'exploration avec le nouvel état
                explore_recursive(
                    game,
                    &mut new_state,
                    path,
                    predefined_cards,
                    depth + 1,
                    max_depth,
                    paths_explored,
                    terminals_reached,
                    verbose,
                )?;

                // Restaurer l'état du jeu
                game.apply_history(&history_before);

                // Retirer la dernière action du chemin
                path.pop();
            }

            return Ok(());
        }

        // 3. Autres types de nœuds (racine) - continuer l'exploration
        _ => {
            // Simplement passer au nœud suivant
            explore_recursive(
                game,
                state,
                path,
                predefined_cards,
                depth + 1,
                max_depth,
                paths_explored,
                terminals_reached,
                verbose,
            )?;
            return Ok(());
        }
    }
}

pub fn explore_all_paths(game: &mut PostFlopGame) -> Result<(), String> {
    println!("=== EXPLORATION SYSTÉMATIQUE DE L'ARBRE DE DÉCISION ===");
    println!(
        "Board initial: {}",
        game.current_board()
            .iter()
            .map(|&c| card_to_string_simple(c))
            .collect::<Vec<_>>()
            .join(" ")
    );

    // Initialiser l'état de jeu
    let mut state = GameState::new();
    let starting_pot = game.tree_config().starting_pot as f64;
    let effective_stack = game.tree_config().effective_stack as f64;

    let root_spot = Spot {
        spot_type: SpotType::Root,
        index: 0,
        player: "flop".to_string(),
        selected_index: -1,
        actions: Vec::new(),
        cards: Vec::new(),
        pot: starting_pot,
        stack: effective_stack,
        equity_oop: 0.0,
        prev_player: None,
    };

    state.spots.push(root_spot);
    select_spot(game, &mut state, 1, true, false)?;

    // Variable pour stocker les cartes prédéfinies pour la turn et la river
    let mut predefined_cards = Vec::new();

    // Compteur pour les chemins explorés et terminaux atteints
    let mut paths_explored = 0;
    let mut terminals_reached = 0;

    // Commencer l'exploration récursive
    let mut path = Vec::new();
    let result = explore_recursive(
        game,
        &mut state,
        &mut path,
        &mut predefined_cards,
        0,
        20,
        &mut paths_explored,
        &mut terminals_reached,
        false, // Mode verbeux désactivé
    );

    // Afficher les statistiques finales
    println!("\n=== RÉSUMÉ DE L'EXPLORATION ===");
    println!("Chemins explorés: {}", paths_explored);
    println!("Nœuds terminaux atteints: {}", terminals_reached);

    // Afficher les cartes prédéfinies qui ont été utilisées
    println!("\n=== CARTES PRÉDÉFINIES UTILISÉES ===");
    for (i, card) in predefined_cards.iter().enumerate() {
        let position = if i == 0 { "Turn" } else { "River" };
        println!(
            "{}: {} (index: {})",
            position,
            card_to_string_simple(card.card_value),
            card.card_index
        );
    }

    // Sauvegarder les résultats en JSON
    match save_exploration_results(game, "results.json") {
        Ok(_) => println!("Résultats de l'exploration sauvegardés en JSON"),
        Err(e) => println!("Erreur lors de la sauvegarde des résultats: {}", e),
    }

    result
}

// Fonctions utilitaires pour convertir des indices en caractères lisibles
pub fn rank_to_char(rank: usize) -> char {
    match rank {
        0 => '2',
        1 => '3',
        2 => '4',
        3 => '5',
        4 => '6',
        5 => '7',
        6 => '8',
        7 => '9',
        8 => 'T',
        9 => 'J',
        10 => 'Q',
        11 => 'K',
        12 => 'A',
        _ => '?',
    }
}

pub fn suit_to_char(suit: usize) -> char {
    match suit {
        0 => 'c',
        1 => 'd',
        2 => 'h',
        3 => 's',
        _ => '?',
    }
}

pub fn card_from_string(card_str: &str) -> Card {
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
        's' => 0,
        'h' => 1,
        'c' => 2,
        'd' => 3,
        _ => panic!("Invalid suit: {}", suit_char),
    };

    rank + (suit * 13)
}

pub fn round_to_decimal_places(value: f32, decimal_places: u32) -> f32 {
    let factor = 10.0_f32.powi(decimal_places as i32);
    (value * factor).round() / factor
}

/// Saves the solver results for the current spot to a binary file if it doesn't already exist
pub fn save_spot_results(
    game: &mut PostFlopGame,
    path_id: &str,
    output_dir: &str,
) -> Result<bool, String> {
    // Create filename from path_id by replacing special chars
    let filename = path_id
        .replace(":", "_")
        .replace(" ", "_")
        .replace(",", "_")
        .replace("-", "_");

    // Create full path including directory
    let full_path = format!("{}/{}.bin", output_dir, filename);

    // Check if file already exists
    if Path::new(&full_path).exists() {
        return Ok(false); // File exists, didn't save
    }

    // Create directory if it doesn't exist
    if !Path::new(output_dir).exists() {
        std::fs::create_dir_all(output_dir)
            .map_err(|e| format!("Failed to create directory {}: {}", output_dir, e))?;
    }

    // Get the solver results
    let buffer = get_results(game);

    // Open file for writing
    let file = File::create(&full_path)
        .map_err(|e| format!("Failed to create file {}: {}", full_path, e))?;

    let mut writer = BufWriter::new(file);

    // Write length of buffer as u64
    let length = buffer.len() as u64;
    writer
        .write_all(&length.to_le_bytes())
        .map_err(|e| format!("Error writing length: {}", e))?;

    // Write buffer data
    for &value in buffer.iter() {
        writer
            .write_all(&value.to_le_bytes())
            .map_err(|e| format!("Error writing data: {}", e))?;
    }

    writer
        .flush()
        .map_err(|e| format!("Error flushing data: {}", e))?;

    Ok(true) // File was saved
}

pub fn display_top_hands(
    game: &mut PostFlopGame,
    num_hands: usize,
    stage_label: &str,
    results: &SpecificResultData,
) -> Result<(), String> {
    println!("\n--- DÉTAIL DES MEILLEURES MAINS ({}) ---", stage_label);

    // Pour chaque joueur (OOP=0, IP=1)
    for player in 0..2 {
        let player_label = if player == 0 { "OOP" } else { "IP" };
        println!("\n{} - Meilleures mains:", player_label);

        // Utiliser les données des résultats fournis
        let equity = &results.equity[player];
        let ev = &results.ev[player];
        let weights = &results.weights[player];

        // Récupérer les mains et les convertir en chaînes
        let hands = if player == 0 {
            &results.oop_cards
        } else {
            &results.ip_cards
        };
        let hand_strings = match holes_to_strings(
            hands
                .iter()
                .map(|&(c1, c2)| (c1 as Card, c2 as Card))
                .collect::<Vec<_>>()
                .as_slice(),
        ) {
            Ok(strings) => strings,
            Err(_) => return Err("Erreur lors de la conversion des mains en chaînes".to_string()),
        };

        // Créer structure pour trier les mains
        struct HandData {
            hand_name: String,
            equity: f64,
            ev: f64,
            weight: f64,
            index: usize, // Ajout de l'index pour retrouver la stratégie
        }

        // Collecter les données pour les mains avec un poids > 0
        let mut hand_data: Vec<HandData> = hand_strings
            .iter()
            .enumerate()
            .filter(|&(i, _)| weights[i] > 0.0)
            .map(|(i, name)| HandData {
                hand_name: name.clone(),
                equity: equity[i],
                ev: ev[i],
                weight: weights[i],
                index: i,
            })
            .collect();

        // Trier les mains par EV décroissant
        hand_data.sort_by(|a, b| b.ev.partial_cmp(&a.ev).unwrap_or(std::cmp::Ordering::Equal));

        // Vérifier s'il s'agit du joueur actuel et si c'est un nœud de décision
        let is_current_player = player == game.current_player();
        let is_decision_node = !game.is_terminal_node() && !game.is_chance_node();

        // Récupérer les actions disponibles et leurs fréquences/EVs si applicable
        let actions = if is_current_player && is_decision_node {
            game.available_actions()
        } else {
            vec![]
        };

        let range_size = hands.len();
        let strategy = if is_current_player && is_decision_node {
            game.strategy()
        } else {
            vec![]
        };

        let action_evs = if is_current_player && is_decision_node {
            game.expected_values_detail(player)
        } else {
            vec![]
        };

        // Afficher l'en-tête du tableau
        println!(
            "{:<6} {:<10} {:<12} {:<10}",
            "Main", "Équité %", "EV (bb)", "Poids %"
        );
        println!("{}", "-".repeat(40));

        // Afficher les N meilleures mains avec leurs détails d'action
        for data in hand_data.iter().take(num_hands) {
            println!(
                "{:<6} {:<10.2} {:<12.2} {:<10.2}",
                data.hand_name,
                data.equity * 100.0,
                data.ev,
                data.weight * 100.0
            );

            // Si c'est un nœud de décision et le joueur actuel, afficher les fréquences et EVs par action
            if is_current_player && is_decision_node && !actions.is_empty() {
                println!("  Actions disponibles:");

                for (action_idx, action) in actions.iter().enumerate() {
                    let action_str = format!("{:?}", action).replace("(", " ").replace(")", "");

                    // Calculer les indices avec précaution
                    let strat_idx = action_idx * range_size + data.index;
                    let ev_idx = action_idx * range_size + data.index;

                    // Récupérer fréquence et EV pour cette action/main
                    let frequency = if strat_idx < strategy.len() {
                        strategy[strat_idx] * 100.0
                    } else {
                        0.0
                    };
                    let action_ev = if ev_idx < action_evs.len() {
                        action_evs[ev_idx]
                    } else {
                        0.0
                    };

                    // Afficher la ligne de détail
                    println!(
                        "    {:<10}: {:<8.2}% (EV: {:.2} bb)",
                        action_str, frequency, action_ev
                    );
                }
                println!(); // Ligne vide entre les mains
            }
        }

        // Afficher l'EV moyenne du joueur
        let total_ev: f64 = hand_data.iter().map(|data| data.ev * data.weight).sum();
        let total_weight: f64 = hand_data.iter().map(|data| data.weight).sum();
        let avg_ev = if total_weight > 0.0 {
            total_ev / total_weight
        } else {
            0.0
        };

        println!("\nEV moyenne {}: {:.2} bb", player_label, avg_ev);

        // Si c'est le joueur actuel, afficher la stratégie globale
        if is_current_player && is_decision_node && !actions.is_empty() {
            println!("\nStratégie globale {}:", player_label);

            for (action_idx, action) in actions.iter().enumerate() {
                let action_str = format!("{:?}", action).replace("(", " ").replace(")", "");

                // Calculer la fréquence moyenne pour cette action
                let mut total_freq = 0.0;
                let mut total_weight = 0.0;
                let norm_weights = game.normalized_weights(player);

                for hand_idx in 0..range_size {
                    let strat_idx = action_idx * range_size + hand_idx;
                    if strat_idx < strategy.len() {
                        total_freq += strategy[strat_idx] * norm_weights[hand_idx];
                        total_weight += norm_weights[hand_idx];
                    }
                }

                let avg_freq = if total_weight > 0.0 {
                    (total_freq / total_weight) * 100.0
                } else {
                    0.0
                };

                // Calculer l'EV moyenne pour cette action
                let mut total_ev = 0.0;
                for hand_idx in 0..range_size {
                    let ev_idx = action_idx * range_size + hand_idx;
                    if ev_idx < action_evs.len() {
                        total_ev += action_evs[ev_idx] * norm_weights[hand_idx];
                    }
                }

                let avg_ev = if total_weight > 0.0 {
                    total_ev / total_weight
                } else {
                    0.0
                };

                println!(
                    "  {:<10}: {:<8.2}% (EV: {:.2} bb)",
                    action_str, avg_freq, avg_ev
                );
            }
        }
    }

    println!("\n");
    Ok(())
}

pub fn format_hand_cards(card_pair: (u8, u8)) -> String {
    format!(
        "{}{}{}{}",
        rank_to_char((card_pair.0 % 13) as usize),
        suit_to_char((card_pair.0 / 13) as usize),
        rank_to_char((card_pair.1 % 13) as usize),
        suit_to_char((card_pair.1 / 13) as usize)
    )
}

// Fonction pour parser une action individuelle
pub fn parse_action(action_str: &str) -> Result<Action, String> {
    match action_str {
        "Fold" => Ok(Action::Fold),
        "Check" => Ok(Action::Check),
        "Call" => Ok(Action::Call),
        s if s.starts_with("Bet") => {
            let amount_str = &s[3..];
            let amount = amount_str
                .parse::<i32>()
                .map_err(|_| format!("Invalid bet amount: {}", amount_str))?;
            Ok(Action::Bet(amount))
        }
        s if s.starts_with("Raise") => {
            let amount_str = &s[5..];
            let amount = amount_str
                .parse::<i32>()
                .map_err(|_| format!("Invalid raise amount: {}", amount_str))?;
            Ok(Action::Raise(amount))
        }
        s if s.starts_with("AllIn") => {
            let amount_str = &s[5..];
            let amount = amount_str
                .parse::<i32>()
                .map_err(|_| format!("Invalid all-in amount: {}", amount_str))?;
            Ok(Action::AllIn(amount))
        }
        _ => Err(format!("Unknown action: {}", action_str)),
    }
}

// Et modifier parse_action_line pour utiliser Action
pub fn parse_action_line(line_str: &[String]) -> Result<Vec<Action>, String> {
    let mut actions = Vec::new();

    for action_str in line_str {
        let action = parse_action(action_str)?;
        actions.push(action);
    }

    Ok(actions)
}

// Dans utils.rs - Version simple sans génération de montants
pub fn remove_lines_simple(action_tree: &mut ActionTree, removed_lines_config: &[Vec<String>]) {
    for line_str in removed_lines_config {
        match parse_action_line(line_str) {
            Ok(action_sequence) => {
                println!("Suppression de la ligne: {:?}", line_str);
                if let Err(e) = action_tree.remove_line(&action_sequence) {
                    eprintln!("Failed to remove line {:?}: {}", line_str, e);
                } else {
                    println!("  ✓ Supprimé");
                }
            }
            Err(e) => {
                eprintln!("Failed to parse action sequence {:?}: {}", line_str, e);
            }
        }
    }
}

// Dans utils.rs
pub fn add_lines_simple(action_tree: &mut ActionTree, added_lines_config: &[Vec<String>]) {
    for line_str in added_lines_config {
        match parse_action_line(line_str) {
            Ok(action_sequence) => {
                println!("Ajout de la ligne: {:?}", line_str);
                if let Err(e) = action_tree.add_line(&action_sequence) {
                    eprintln!("Failed to add line {:?}: {}", line_str, e);
                } else {
                    println!("  ✓ Ligne ajoutée avec succès");
                }
            }
            Err(e) => {
                eprintln!("Failed to parse action sequence {:?}: {}", line_str, e);
            }
        }
    }
}

// Dans utils.rs
// Dans utils.rs - Remplacer remove_all_lines par :
pub fn remove_all_lines(action_tree: &mut ActionTree) -> Result<(), String> {
    println!("\n=== SUPPRESSION DE TOUTES LES LIGNES ===");

    let mut removed_count = 0;
    let max_attempts = 1000; // Limite de sécurité

    // Continuer à supprimer jusqu'à ce qu'il n'y ait plus rien à supprimer
    for attempt in 0..max_attempts {
        let invalid_terminals = action_tree.invalid_terminals();

        if invalid_terminals.is_empty() {
            break;
        }

        // Prendre la première ligne et essayer de la supprimer
        let line_to_remove = &invalid_terminals[0];

        match action_tree.remove_line(line_to_remove) {
            Ok(()) => {
                removed_count += 1;
                println!("  ✓ Ligne {} supprimée", removed_count);
            }
            Err(e) => {
                println!("  ✗ Erreur: {}", e);
                break;
            }
        }
    }

    if removed_count == 0 {
        println!("Aucune ligne à supprimer - l'arbre est déjà minimal");
    } else {
        println!("=== {} LIGNES SUPPRIMÉES ===", removed_count);
    }

    Ok(())
}
