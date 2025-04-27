// Types pour représenter les spots (nœuds) dans l'arbre de jeu
#[derive(Debug, Clone)]
pub enum SpotType {
    Root,
    Player,
    Chance,
    Terminal,
}

#[derive(Debug, Clone)]
pub struct Action {
    pub index: usize,
    pub name: String,
    pub amount: String,
    pub is_selected: bool,
}

#[derive(Debug, Clone)]
pub struct SpotCard {
    pub card: usize,
    pub is_selected: bool,
    pub is_dead: bool,
}

#[derive(Debug, Clone)]
pub struct Spot {
    pub spot_type: SpotType,
    pub index: usize,
    pub player: String,
    pub selected_index: i32,
    pub actions: Vec<Action>,
    pub cards: Vec<SpotCard>,
    pub pot: f64,
    pub stack: f64,
    pub equity_oop: f64,
}

// Gestionnaire d'état global (similaire aux refs de Vue)
pub struct GameState {
    pub spots: Vec<Spot>,
    pub selected_spot_index: i32,
    pub selected_chance_index: i32,
    pub is_dealing: bool,
}

impl GameState {
    pub fn new() -> Self {
        Self {
            spots: Vec::new(),
            selected_spot_index: -1,
            selected_chance_index: -1,
            is_dealing: false,
        }
    }
}

/// Fonction pour sélectionner un spot et mettre à jour l'état du jeu
/// Similaire à selectSpotFront dans ResultNav.vue
pub fn select_spot(
    game: &mut PostFlopGame,
    state: &mut GameState,
    spot_index: usize,
    need_splice: bool,
    from_deal: bool,
) -> Result<(), String> {
    // Si spot_index est 0, sélectionner l'index 1 à la place
    if spot_index == 0 {
        return select_spot(game, state, 1, true, false);
    }

    // Temporairement stocker les indices pour éviter des mises à jour inutiles
    let mut selected_spot_index_tmp = state.selected_spot_index;
    let mut selected_chance_index_tmp = state.selected_chance_index;

    // Mise à jour des indices basée sur la logique du frontend
    if !need_splice && state.spots[spot_index].spot_type == SpotType::Chance {
        selected_chance_index_tmp = spot_index as i32;
        if selected_spot_index_tmp < (spot_index + 1) as i32 {
            selected_spot_index_tmp = (spot_index + 1) as i32;
        }
    } else {
        selected_spot_index_tmp = spot_index as i32;
        if (spot_index as i32) <= selected_chance_index_tmp {
            selected_chance_index_tmp = -1;
        }
    }

    // Déterminer jusqu'où appliquer l'historique
    let end_index = if selected_chance_index_tmp == -1 {
        spot_index
    } else {
        selected_chance_index_tmp as usize
    };

    // Revenir à la racine pour appliquer l'historique
    game.back_to_root();

    // Collecter les actions à jouer à partir de l'historique des spots
    let mut action_history = Vec::new();

    for i in 1..end_index {
        let spot = &state.spots[i];
        let action = if spot.spot_type == SpotType::Chance && spot.selected_index >= 0 {
            // Pour les nœuds de chance (turn/river), utiliser la carte sélectionnée
            let card = spot.selected_index as usize;
            Action::Chance(card as u8)
        } else if spot.spot_type == SpotType::Player && spot.selected_index >= 0 {
            // Pour les nœuds de joueur, utiliser l'action sélectionnée
            let action = &spot.actions[spot.selected_index as usize];
            match action.name.as_str() {
                "Fold" => Action::Fold,
                "Check" => Action::Check,
                "Call" => Action::Call,
                name if name.starts_with("Bet") => {
                    let amount: i32 = action.amount.parse().unwrap_or(0);
                    Action::Bet(amount)
                }
                name if name.starts_with("Raise") => {
                    let amount: i32 = action.amount.parse().unwrap_or(0);
                    Action::Raise(amount)
                }
                name if name.starts_with("AllIn") => {
                    let amount: i32 = action.amount.parse().unwrap_or(0);
                    Action::AllIn(amount)
                }
                _ => return Err(format!("Action non reconnue: {}", action.name)),
            }
        } else {
            continue;
        };

        action_history.push(action);
    }

    // Appliquer l'historique des actions
    game.apply_history(&action_history)?;

    // Déterminer le type de joueur actuel et le nombre d'actions
    let current_player = if game.is_terminal_node() {
        "terminal"
    } else if game.is_chance_node() {
        "chance"
    } else if game.current_player() == 0 {
        "oop"
    } else {
        "ip"
    };

    let num_actions = if game.is_terminal_node() || game.is_chance_node() {
        0
    } else {
        game.available_actions().len()
    };

    // Obtenir les résultats pour le nœud actuel
    game.cache_normalized_weights();
    let results = get_specific_result(game, current_player, num_actions);

    // Si need_splice est true, on met à jour les spots suivants
    if need_splice {
        if game.is_terminal_node() {
            // Mettre à jour avec un nœud terminal
            update_spots_terminal(game, state, spot_index, &results);
        } else if game.is_chance_node() {
            // Mettre à jour avec un nœud de chance (turn/river)
            update_spots_chance(game, state, spot_index, &results);
        } else {
            // Mettre à jour avec un nœud de joueur normal
            update_spots_player(game, state, spot_index, current_player);
        }
    }

    // Mettre à jour les taux de stratégie globaux et par main
    update_strategy_rates(game, state, spot_index, current_player, &results);

    // Mettre à jour les indices définitifs
    state.selected_spot_index = selected_spot_index_tmp;
    state.selected_chance_index = selected_chance_index_tmp;

    Ok(())
}

// Fonction pour mettre à jour les spots avec un nœud terminal
fn update_spots_terminal(
    game: &mut PostFlopGame,
    state: &mut GameState,
    spot_index: usize,
    results: &Box<[f64]>,
) {
    let prev_spot = &state.spots[spot_index - 1];

    // Déterminer l'équité OOP (fold, showdown ou skipped)
    let equity_oop = if prev_spot.actions[prev_spot.selected_index as usize].name == "Fold" {
        if prev_spot.player == "oop" {
            0.0
        } else {
            1.0
        }
    } else if results[2] != 0.0 {
        // isEmpty
        -1.0
    } else {
        // Calculer l'équité moyenne
        let oop_range_size = game.private_cards(0).len();
        let oop_equity_offset = 3 + 2 * oop_range_size;
        let oop_equity = &results[oop_equity_offset..(oop_equity_offset + oop_range_size)];

        // Calculer équité moyenne pondérée
        let weights = game.normalized_weights(0);
        let mut total_equity = 0.0;
        let mut total_weight = 0.0;

        for i in 0..oop_range_size {
            total_equity += oop_equity[i] * weights[i] as f64;
            total_weight += weights[i] as f64;
        }

        if total_weight > 0.0 {
            total_equity / total_weight
        } else {
            0.0
        }
    };

    // Calculer le pot final
    let total_bet = game.total_bet_amount();
    let pot_base = game.tree_config().starting_pot as f64;
    let bet_sum = total_bet[0] + total_bet[1];
    let pot_size = pot_base + bet_sum as f64;

    // Créer le spot terminal
    let terminal_spot = Spot {
        spot_type: SpotType::Terminal,
        index: spot_index,
        player: "end".to_string(),
        selected_index: -1,
        actions: Vec::new(),
        cards: Vec::new(),
        pot: pot_size,
        stack: 0.0,
        equity_oop,
    };

    // Remplacer tous les spots à partir de l'index actuel
    state.spots.truncate(spot_index);
    state.spots.push(terminal_spot);
}

// Fonction pour mettre à jour les spots avec un nœud de chance
fn update_spots_chance(
    game: &mut PostFlopGame,
    state: &mut GameState,
    spot_index: usize,
    results: &Box<[f64]>,
) {
    let prev_spot = &state.spots[spot_index - 1];

    // Déterminer si c'est turn ou river
    let is_turn = game.current_board().len() == 3;
    let player_name = if is_turn {
        "turn".to_string()
    } else {
        "river".to_string()
    };

    // Obtenir les cartes disponibles
    let possible_cards = game.possible_cards();

    // Calculer le pot actuel
    let total_bet = game.total_bet_amount();
    let pot_base = game.tree_config().starting_pot as f64;
    let pot_size = pot_base + 2.0 * total_bet[0] as f64;
    let stack = game.tree_config().effective_stack as f64 - total_bet[0] as f64;

    // Créer le nœud de chance
    let mut cards = Vec::new();
    for i in 0..52 {
        let is_dead = (possible_cards & (1u64 << i)) == 0;
        cards.push(SpotCard {
            card: i,
            is_selected: false,
            is_dead,
        });
    }

    let chance_spot = Spot {
        spot_type: SpotType::Chance,
        index: spot_index,
        player: player_name,
        selected_index: -1,
        actions: Vec::new(),
        cards,
        pot: pot_size,
        stack,
        equity_oop: 0.0,
    };

    // Créer un nœud joueur OOP pour suivre le nœud de chance
    let mut player_actions = Vec::new();
    let available_actions = game.available_actions();

    // Simuler la sélection d'une carte (peu importe laquelle)
    // pour voir quelles actions seront disponibles après
    if let Some(card_action) = available_actions
        .iter()
        .position(|a| matches!(a, Action::Chance(_)))
    {
        game.play(card_action);

        let next_actions = game.available_actions();
        for (i, action) in next_actions.iter().enumerate() {
            let (name, amount) = match action {
                Action::Fold => ("Fold", "0"),
                Action::Check => ("Check", "0"),
                Action::Call => ("Call", "0"),
                Action::Bet(amt) => ("Bet", amt.to_string()),
                Action::Raise(amt) => ("Raise", amt.to_string()),
                Action::AllIn(amt) => ("AllIn", amt.to_string()),
                _ => continue,
            };

            player_actions.push(Action {
                index: i,
                name: name.to_string(),
                amount: amount.to_string(),
                is_selected: false,
            });
        }

        // Revenir en arrière
        game.back_to_root();
        game.apply_history(&available_actions[0..spot_index])
            .unwrap_or(());
    }

    // Créer le nœud joueur qui suivra le nœud de chance
    let player_spot = Spot {
        spot_type: SpotType::Player,
        index: spot_index + 1,
        player: "oop".to_string(), // Après le turn/river, c'est toujours OOP qui joue d'abord
        selected_index: -1,
        actions: player_actions,
        cards: Vec::new(),
        pot: pot_size,
        stack,
        equity_oop: 0.0,
    };

    // Mettre à jour les spots
    state.spots.truncate(spot_index);
    state.spots.push(chance_spot);
    state.spots.push(player_spot);
}

// Fonction pour mettre à jour les spots avec un nœud joueur
fn update_spots_player(
    game: &mut PostFlopGame,
    state: &mut GameState,
    spot_index: usize,
    current_player: &str,
) {
    let prev_spot = &state.spots[spot_index - 1];

    // Déterminer le joueur (oop ou ip)
    let player = if prev_spot.player == "oop" {
        "ip"
    } else {
        "oop"
    };

    // Obtenir les actions disponibles
    let mut player_actions = Vec::new();
    let available_actions = game.available_actions();

    for (i, action) in available_actions.iter().enumerate() {
        let (name, amount) = match action {
            Action::Fold => ("Fold", "0"),
            Action::Check => ("Check", "0"),
            Action::Call => ("Call", "0"),
            Action::Bet(amt) => ("Bet", amt.to_string()),
            Action::Raise(amt) => ("Raise", amt.to_string()),
            Action::AllIn(amt) => ("AllIn", amt.to_string()),
            _ => continue,
        };

        player_actions.push(Action {
            index: i,
            name: name.to_string(),
            amount: amount.to_string(),
            is_selected: false,
        });
    }

    // Calculer le pot actuel
    let total_bet = game.total_bet_amount();
    let pot_base = game.tree_config().starting_pot as f64;
    let pot_size = pot_base + total_bet[0] as f64 + total_bet[1] as f64;
    let stack = game.tree_config().effective_stack as f64
        - total_bet[if player == "oop" { 0 } else { 1 }] as f64;

    // Créer le nœud joueur
    let player_spot = Spot {
        spot_type: SpotType::Player,
        index: spot_index,
        player: player.to_string(),
        selected_index: -1,
        actions: player_actions,
        cards: Vec::new(),
        pot: pot_size,
        stack,
        equity_oop: 0.0,
    };

    // Mettre à jour les spots
    state.spots.truncate(spot_index);
    state.spots.push(player_spot);
}

// Fonction pour mettre à jour les taux de stratégie
fn update_strategy_rates(
    game: &mut PostFlopGame,
    state: &mut GameState,
    spot_index: usize,
    current_player: &str,
    results: &Box<[f64]>,
) {
    // Seulement pour les nœuds de joueur
    if current_player != "oop" && current_player != "ip" {
        return;
    }

    let spot = &mut state.spots[spot_index];
    if spot.spot_type != SpotType::Player {
        return;
    }

    // Vérifier si la range est vide
    let is_empty = results[2] != 0.0;
    if is_empty {
        return;
    }

    // Calculer les taux de stratégie
    let player_idx = if current_player == "oop" { 0 } else { 1 };
    let range_size = game.private_cards(player_idx).len();

    let strategy = game.strategy();
    let weights = game.normalized_weights(player_idx);

    // Calculer les taux moyens par action
    for (i, action) in spot.actions.iter_mut().enumerate() {
        let mut total_freq = 0.0;
        let mut total_weight = 0.0;

        for hand_idx in 0..range_size {
            let strat_idx = hand_idx + i * range_size;
            if strat_idx < strategy.len() {
                total_freq += strategy[strat_idx] * weights[hand_idx];
                total_weight += weights[hand_idx];
            }
        }

        // Calculer et stocker le taux moyen
        let avg_freq = if total_weight > 0.0 {
            total_freq / total_weight * 100.0
        } else {
            0.0
        };

        // Mettre à jour le taux dans l'action (simulé par un affichage)
        println!("Action {} : {:.2}%", action.name, avg_freq);
    }
}
