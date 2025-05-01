// use crate::log_game_state;
use crate::utils::actions_after;
use crate::utils::get_current_actions_string;
use crate::utils::get_specific_result;
use crate::weighted_average;
use crate::Card;
use crate::PostFlopGame;
use crate::SpecificResultData;

// Types pour représenter les spots (nœuds) dans l'arbre de jeu
#[derive(Debug, Clone, PartialEq)]
pub enum SpotType {
    Root,
    Player,
    Chance,
    Terminal,
}

pub struct SpotSelectionResult {
    pub selected_spot: Option<Spot>,
    pub selected_chance: Option<Spot>,
    pub current_board: Vec<Card>,
    pub results: Box<[f64]>,
    pub chance_reports: Option<Box<[f64]>>,
    pub total_bet_amount: [f64; 2],
}

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub index: usize,
    pub name: String,
    pub amount: String,
    pub is_selected: bool,
    pub rate: f64, // Taux pour les statistiques
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
    pub prev_player: Option<String>, // Nouveau champ
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
    pub results_empty: bool, // Indique si les résultats ont été calculés
    pub equity_oop: f64,     // L'équité du joueur OOP
    pub total_bet_amount_appended: [i32; 2], // Montants des mises [OOP, IP]
    pub can_chance_reports: bool, // Indique si les rapports de chance sont disponibles
    pub last_results: Option<Box<[f64]>>,
}

impl GameState {
    pub fn new() -> Self {
        Self {
            spots: Vec::new(),
            selected_spot_index: -1,
            selected_chance_index: -1,
            is_dealing: false,
            results_empty: true,
            equity_oop: 0.0,
            total_bet_amount_appended: [0, 0],
            can_chance_reports: false,
            last_results: None,
        }
    }

    pub fn log_spot(&self, index: usize) {
        if let Some(spot) = self.spots.get(index) {
            println!(
                "Spot #{} - Type: {:?}, Player: {}, Index: {}",
                index, spot.spot_type, spot.player, spot.index
            );
            println!("  Selected index: {}", spot.selected_index);

            if !spot.actions.is_empty() {
                println!("  Actions:");
                for (j, action) in spot.actions.iter().enumerate() {
                    println!(
                        "    {}: {} {} (selected: {}, rate: {:.2})",
                        j, action.name, action.amount, action.is_selected, action.rate
                    );
                }
            }

            if spot.spot_type == SpotType::Chance {
                let selected_cards: Vec<_> = spot
                    .cards
                    .iter()
                    .filter(|c| c.is_selected)
                    .map(|c| c.card)
                    .collect();
                let dead_cards_count = spot.cards.iter().filter(|c| c.is_dead).count();
                println!(
                    "  Cards: {} cards, {} dead, Selected: {:?}",
                    spot.cards.len(),
                    dead_cards_count,
                    selected_cards
                );
            }

            println!(
                "  Pot: {:.2}, Stack: {:.2}, Equity OOP: {:.2}",
                spot.pot, spot.stack, spot.equity_oop
            );
            println!("  Previous player: {:?}", spot.prev_player);
        } else {
            println!("Spot à l'index {} non trouvé", index);
        }
    }

    pub fn log_spots(&self, prefix: &str) {
        println!("\n=== {} - SPOTS STATE ===", prefix);
        println!("Total spots: {}", self.spots.len());
        println!("Selected spot index: {}", self.selected_spot_index);
        println!("Selected chance index: {}", self.selected_chance_index);

        for (i, spot) in self.spots.iter().enumerate() {
            println!(
                "Spot #{} - Type: {:?}, Player: {}, Index: {}",
                i, spot.spot_type, spot.player, spot.index
            );
            println!("  Selected index: {}", spot.selected_index);

            if !spot.actions.is_empty() {
                println!("  Actions:");
                for (j, action) in spot.actions.iter().enumerate() {
                    println!(
                        "    {}: {} {} (selected: {}, rate: {:.2})",
                        j, action.name, action.amount, action.is_selected, action.rate
                    );
                }
            }

            if spot.spot_type == SpotType::Chance {
                let selected_cards: Vec<_> = spot
                    .cards
                    .iter()
                    .filter(|c| c.is_selected)
                    .map(|c| c.card)
                    .collect();
                let dead_cards_count = spot.cards.iter().filter(|c| c.is_dead).count();
                println!(
                    "  Cards: {} cards, {} dead, Selected: {:?}",
                    spot.cards.len(),
                    dead_cards_count,
                    selected_cards
                );
            }

            println!(
                "  Pot: {:.2}, Stack: {:.2}, Equity OOP: {:.2}",
                spot.pot, spot.stack, spot.equity_oop
            );
            println!("  Previous player: {:?}", spot.prev_player);
        }

        println!("===============================\n");
    }

    pub fn log_state(&self, prefix: &str) {
        println!("\n=== {} - GAME STATE ===", prefix);
        println!("Selected spot index: {}", self.selected_spot_index);
        println!("Selected chance index: {}", self.selected_chance_index);
        println!("Is dealing: {}", self.is_dealing);
        println!("Results empty: {}", self.results_empty);
        println!("Equity OOP: {:.4}", self.equity_oop);
        println!(
            "Total bet amount: OOP={}, IP={}",
            self.total_bet_amount_appended[0], self.total_bet_amount_appended[1]
        );
        println!("Can chance reports: {}", self.can_chance_reports);
        println!("Has last results: {}", self.last_results.is_some());
        // self.log_spots(prefix);
    }
}

/// Function to navigate to a specific node in the game tree
/// Based on selectSpotFront from ResultNav.vue
pub fn select_spot(
    game: &mut PostFlopGame,
    state: &mut GameState,
    spot_index: usize,
    need_splice: bool,
    from_deal: bool,
) -> Result<SpecificResultData, String> {
    println!(
        "select_spot() - spot_index: {}, need_splice: {}, from_deal: {}",
        spot_index, need_splice, from_deal
    );

    // Skip if already at the selected spot and not from a deal action
    if !need_splice
        && (spot_index == state.selected_spot_index as usize && !from_deal
            || spot_index == state.selected_chance_index as usize
            || (state.spots[spot_index].spot_type == SpotType::Chance
                && state.selected_chance_index != -1
                && state.spots[state.selected_chance_index as usize].selected_index == -1
                && spot_index > state.selected_chance_index as usize))
    {
        return Err("Spot déjà sélectionné".to_string());
    }

    // If spot_index is 0, select spot 1 instead
    if spot_index == 0 {
        return select_spot(game, state, 1, true, false);
    }

    // Store temporary values for indices to avoid unnecessary ref updates
    let mut selected_spot_index_tmp = state.selected_spot_index;
    let mut selected_chance_index_tmp = state.selected_chance_index;

    if from_deal {
        selected_chance_index_tmp = process_from_deal(game, state)?;
    }

    // Update selected indices based on spot type
    if !need_splice && state.spots[spot_index].spot_type == SpotType::Chance {
        selected_chance_index_tmp = spot_index as i32;
        if selected_spot_index_tmp < (spot_index + 1) as i32 {
            selected_spot_index_tmp = (spot_index + 1) as i32;
        }
    } else {
        selected_spot_index_tmp = spot_index as i32;
        if (spot_index as i32) <= selected_chance_index_tmp {
            selected_chance_index_tmp = -1;
        } else if selected_chance_index_tmp == -1 {
            // Find first chance node with no selection before spot_index
            for i in 0..spot_index {
                if state.spots[i].spot_type == SpotType::Chance
                    && state.spots[i].selected_index == -1
                {
                    selected_chance_index_tmp = i as i32;
                    break;
                }
            }
        }
    }

    // Determine end index for history application
    let end_index = if selected_chance_index_tmp == -1 {
        selected_spot_index_tmp as usize
    } else {
        selected_chance_index_tmp as usize
    };

    // Build history array from spots
    let mut history: Vec<usize> = Vec::new();
    for i in 1..end_index {
        if state.spots[i].selected_index != -1 {
            history.push(state.spots[i].selected_index as usize);
        }
    }

    game.apply_history(&history);

    // Get current player type and number of actions
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

    let results = get_specific_result(game, current_player, num_actions)?;

    // Check if results are empty
    state.results_empty = results.is_empty;

    let mut append_array: Vec<i32> = Vec::new();

    if selected_chance_index_tmp != -1 {
        for i in selected_chance_index_tmp as usize..selected_spot_index_tmp as usize {
            append_array.push(state.spots[i].selected_index);
        }
    }

    // Convertir en Rust-friendly array
    let append: Vec<usize> = append_array
        .iter()
        .map(|&x| if x < 0 { 0 } else { x as usize })
        .collect();

    // Obtenir les actions après le skip des chances
    let next_actions_str = actions_after(game, &append);

    // Vérifier si on peut avoir des chance reports
    let can_chance_reports = selected_chance_index_tmp != -1
        && state.spots[(selected_chance_index_tmp + 3) as usize..selected_spot_index_tmp as usize]
            .iter()
            .all(|spot| spot.spot_type != SpotType::Chance)
        && next_actions_str != "chance";

    state.can_chance_reports = can_chance_reports;

    // Obtenir les chance reports si possible
    // let mut chance_reports = None;
    if can_chance_reports {
        // Ici, on implémenterait l'équivalent de getChanceReports
        // Pour l'instant on laisse à None
        println!("Chance reports disponibles mais non implémentés");
    }

    // Update total bet amount
    state.total_bet_amount_appended = game.total_bet_amount();

    // Update spots if needed (splice)
    if need_splice {
        // Remove all spots after the current one
        state.spots.truncate(spot_index);

        if game.is_terminal_node() || next_actions_str == "terminal" {
            // Create a terminal spot
            splice_spots_terminal(game, state, spot_index)?;
        } else if game.is_chance_node() || next_actions_str == "chance" {
            splice_spots_chance(game, state, spot_index)?;
            selected_spot_index_tmp += 1;

            //logger state
        } else {
            // Create a player spot
            splice_spots_player(state, spot_index, next_actions_str)?;
        }
    }

    // Update action rates for selected player spot
    if let Some(spot) = state.spots.get_mut(selected_spot_index_tmp as usize) {
        if spot.spot_type == SpotType::Player && selected_chance_index_tmp == -1 {
            let player_index = if spot.player == "oop" { 0 } else { 1 };

            if !state.results_empty {
                // update_action_rates(spot, game, player_index);
            }
        }
    }

    // Update indices after all processing
    state.selected_spot_index = selected_spot_index_tmp;
    state.selected_chance_index = selected_chance_index_tmp;
    state.is_dealing = false;

    Ok(results)
}

/// Helper function to update action rates for a player spot
fn update_action_rates(spot: &mut Spot, game: &PostFlopGame, player_index: usize) {
    println!("update_action_rates() - spot_type: {:?}", spot.spot_type);

    // if game.is_chance_node() || game.is_terminal_node() {
    if spot.spot_type != SpotType::Player {
        println!("Skipping update_action_rates for non-player node");
        return;
    }

    // println!("Updating action rates for player index");
    let strategy = game.strategy();
    // println!("update_action_rates() - after strategy()");
    let weights = game.normalized_weights(player_index);
    let num_hands = weights.len();

    for (i, action) in spot.actions.iter_mut().enumerate() {
        let mut total_weight = 0.0;
        let mut total_freq = 0.0;

        for hand_idx in 0..num_hands {
            let weight = weights[hand_idx];
            total_weight += weight as f64;

            let strat_idx = hand_idx + i * num_hands;
            if strat_idx < strategy.len() {
                total_freq += strategy[strat_idx] as f64 * weight as f64;
            }
        }

        action.rate = if total_weight > 0.0 {
            (total_freq / total_weight) as f64
        } else {
            0.0
        };

        println!("Action: {}, Rate: {}", action.name, action.rate);
    }
}

/// Fonction process_from_deal qui gère les mises à jour spéciales après un deal
/// Basée directement sur le code Vue dans le bloc if (fromDeal) de selectSpotResult
fn process_from_deal(game: &mut PostFlopGame, state: &mut GameState) -> Result<i32, String> {
    // Chercher l'index du nœud "river" (prochain nœud de chance après le nœud actuel)
    let selected_chance_idx = state.selected_chance_index as usize;
    let find_river_index = state
        .spots
        .iter()
        .skip(selected_chance_idx + 3) // Skip 3 spots après le nœud de chance actuel
        .position(|spot| spot.spot_type == SpotType::Chance);

    let river_index = if let Some(idx) = find_river_index {
        idx + selected_chance_idx + 3
    } else {
        usize::MAX // Équivalent à -1 dans le code Vue
    };

    // Obtenir le nœud river si présent
    let river_spot_exists = river_index != usize::MAX && river_index < state.spots.len();

    // IMPORTANT: Réinitialiser le selected_chance_index à -1 comme dans le frontend
    // C'est la clé de la correction du bug
    let new_selected_chance_index = -1;

    // Si un nœud river existe, mettre à jour les cartes mortes
    if river_spot_exists {
        // Construire l'historique jusqu'au nœud river
        game.back_to_root();
        let mut history = Vec::new();
        for i in 1..river_index {
            if state.spots[i].selected_index != -1 {
                history.push(state.spots[i].selected_index as usize);
            }
        }
        game.apply_history(&history);

        // Obtenir les cartes possibles
        let possible_cards = game.possible_cards();

        // Mettre à jour les cartes mortes dans le nœud river
        let river_spot = &mut state.spots[river_index];
        for (i, card) in river_spot.cards.iter_mut().enumerate() {
            let is_dead = (possible_cards & (1u64 << i)) == 0;
            card.is_dead = is_dead;

            // Si la carte sélectionnée est maintenant morte, la désélectionner
            if river_spot.selected_index == i as i32 && is_dead {
                card.is_selected = false;
                river_spot.selected_index = -1;
            }
        }
    }

    // Vérifier si le dernier spot est terminal et mettre à jour son équité
    let river_skipped = river_spot_exists && state.spots[river_index].selected_index == -1;
    let last_spot_idx = state.spots.len() - 1;

    if !river_skipped
        && last_spot_idx < state.spots.len()
        && state.spots[last_spot_idx].spot_type == SpotType::Terminal
        && state.spots[last_spot_idx].equity_oop != 0.0
        && state.spots[last_spot_idx].equity_oop != 1.0
    {
        // Construire l'historique jusqu'au dernier nœud
        game.back_to_root();
        let mut history = Vec::new();
        for i in 1..last_spot_idx {
            if state.spots[i].selected_index != -1 {
                history.push(state.spots[i].selected_index as usize);
            }
        }
        game.apply_history(&history);

        // Obtenir les résultats
        if let Ok(results) = get_specific_result(game, "terminal", 0) {
            if !results.is_empty {
                // Mettre à jour l'équité OOP du spot terminal
                let equity_oop = weighted_average(
                    &results.equity[0]
                        .iter()
                        .map(|&x| x as f32)
                        .collect::<Vec<_>>(),
                    &results.normalizer[0]
                        .iter()
                        .map(|&x| x as f32)
                        .collect::<Vec<_>>(),
                ) as f64;

                state.spots[last_spot_idx].equity_oop = equity_oop;
            } else {
                state.spots[last_spot_idx].equity_oop = -1.0;
            }
        }
    }

    // Retourner le nouveau selected_chance_index pour que select_spot le mette à jour
    Ok(new_selected_chance_index)
}

/// Calculate average equity for a player
fn calculate_average_equity(game: &PostFlopGame, player_index: usize) -> f64 {
    let equity = game.equity(player_index);
    let weights = game.normalized_weights(player_index);

    let mut total_equity = 0.0;
    let mut total_weight = 0.0;

    for i in 0..weights.len() {
        let weight = weights[i] as f64;
        total_equity += equity[i] as f64 * weight;
        total_weight += weight;
    }

    if total_weight > 0.0 {
        total_equity / total_weight
    } else {
        0.0
    }
}

/// Fonction pour mettre à jour les spots avec un nœud terminal
/// Reproduction fidèle de spliceSpotsTerminal dans ResultNav.vue
fn splice_spots_terminal(
    game: &mut PostFlopGame,
    state: &mut GameState,
    spot_index: usize,
) -> Result<(), String> {
    // Récupérer le spot précédent et son action sélectionnée
    let prev_spot = &state.spots[spot_index - 1];
    let prev_action = &prev_spot.actions[prev_spot.selected_index as usize];

    // Vérifier si un noeud de chance est sauté
    let chance_index = state.selected_chance_index;
    let chance_skipped =
        chance_index != -1 && state.spots[chance_index as usize].selected_index == -1;

    // Déterminer l'équité OOP en fonction de l'action précédente
    let equity_oop = if prev_action.name == "Fold" {
        // Si c'est un fold, l'équité dépend du joueur qui a foldé
        if prev_spot.player == "oop" {
            0.0
        } else {
            1.0
        }
    } else if chance_skipped || state.results_empty {
        // Si chance skippée ou résultats vides, équité indéterminée
        -1.0
    } else {
        // Sinon calculer l'équité moyenne (équivalent à average(results.equity[0], results.normalizer[0]))
        state.equity_oop
    };

    // Calculer le pot final (somme des mises des deux joueurs)
    let bet_sum = state.total_bet_amount_appended[0] + state.total_bet_amount_appended[1];
    let final_pot = game.tree_config().starting_pot as f64 + bet_sum as f64;

    // Créer le nouveau spot terminal
    let new_terminal_spot = Spot {
        spot_type: SpotType::Terminal,
        index: spot_index,
        player: "end".to_string(),
        selected_index: -1,
        cards: Vec::new(),
        actions: Vec::new(),
        pot: final_pot,
        stack: 0.0, // Le stack n'est pas utilisé dans les spots terminaux
        equity_oop,
        prev_player: Some(prev_spot.player.clone()),
    };

    // Remplacer tous les spots à partir de l'index actuel
    state.spots.truncate(spot_index);
    state.spots.push(new_terminal_spot);

    Ok(())
}

fn splice_spots_chance(
    game: &mut PostFlopGame,
    state: &mut GameState,
    spot_index: usize,
) -> Result<(), String> {
    let prev_spot = &state.spots[spot_index - 1];

    let turn_spot = state
        .spots
        .iter()
        .take(spot_index)
        .find(|spot| spot.player == "turn");

    let mut append_array = Vec::new();
    if state.selected_chance_index != -1 {
        // Ajouter les indices sélectionnés entre le nœud de chance sélectionné et l'index actuel
        for i in state.selected_chance_index as usize..spot_index {
            append_array.push(state.spots[i].selected_index);
        }
    }

    // Déterminer les cartes possibles (comme possibleCards dans le frontend)
    let mut possible_cards = 0u64;
    // Si nous n'avons pas de turn, ou si le turn n'a pas de carte sélectionnée, on peut calculer
    if !(turn_spot.is_some()
        && turn_spot.unwrap().spot_type == SpotType::Chance
        && turn_spot.unwrap().selected_index == -1)
    {
        possible_cards = game.possible_cards();
    }

    // Ajouter -1 au append (comme dans le frontend)
    append_array.push(-1);

    // Obtenir les actions disponibles après ce append (comme nextActionsStr)
    let next_actions_str = actions_after(game, &append_array);
    let next_actions: Vec<&str> = next_actions_str.split('/').collect();

    // Mettre à jour canChanceReports (comme dans le frontend)
    let can_chance_reports = state.selected_chance_index == -1;

    // Si nous avons besoin de rapports de chance et que selectedChanceIndex est -1
    if can_chance_reports {
        // Obtenir les rapports de chance (équivalent à getChanceReports)
        println!(
            "Obtention des rapports de chance pour {} actions",
            next_actions.len()
        );
    }

    // Modifier le tableau des spots par splice (comme dans le frontend)
    // Créer le nouveau spot de chance (turn ou river)
    let new_chance_spot = Spot {
        spot_type: SpotType::Chance,
        index: spot_index,
        player: if turn_spot.is_some() {
            "river".to_string()
        } else {
            "turn".to_string()
        },
        selected_index: -1,
        cards: (0..52)
            .map(|i| SpotCard {
                card: i,
                is_selected: false,
                is_dead: (possible_cards & (1u64 << i)) == 0,
            })
            .collect(),
        actions: Vec::new(),
        pot: game.tree_config().starting_pot as f64 + 2.0 * game.total_bet_amount()[0] as f64,
        stack: game.tree_config().effective_stack as f64 - game.total_bet_amount()[0] as f64,
        equity_oop: 0.0,
        prev_player: Some(prev_spot.player.clone()),
    };

    // Créer le spot de joueur (toujours OOP après un nœud de chance)
    let mut player_actions = Vec::new();
    for (i, action) in next_actions.iter().enumerate() {
        let parts: Vec<&str> = action.split(':').collect();
        let name = parts[0].to_string();
        let amount = if parts.len() > 1 {
            parts[1].to_string()
        } else {
            "0".to_string()
        };

        player_actions.push(Action {
            index: i,
            name,
            amount,
            is_selected: false,
            rate: -1.0,
        });
    }

    let new_player_spot = Spot {
        spot_type: SpotType::Player,
        index: spot_index + 1,
        player: "oop".to_string(), // Toujours OOP qui joue après un nœud de chance
        selected_index: -1,
        cards: Vec::new(),
        actions: player_actions,
        pot: new_chance_spot.pot,
        stack: new_chance_spot.stack,
        equity_oop: 0.0,
        prev_player: Some(new_chance_spot.player.clone()), // Correction: ajout du champ manquant
    };

    // Faire le splice comme dans le frontend
    state.spots.truncate(spot_index);
    state.spots.push(new_chance_spot);
    state.spots.push(new_player_spot);

    // Incrémenter selectedSpotIndexTmp comme dans le frontend
    if state.selected_spot_index as usize == spot_index {
        state.selected_spot_index += 1;
    }

    // Mettre à jour selectedChanceIndexTmp si nécessaire
    if state.selected_chance_index == -1 {
        state.selected_chance_index = spot_index as i32;
    }

    // state.log_spots("");

    Ok(())
}

/// Fonction pour mettre à jour les spots avec un nœud de joueur
/// Reproduction fidèle de spliceSpotsPlayer dans ResultNav.vue
fn splice_spots_player(
    state: &mut GameState,
    spot_index: usize,
    actions_str: String,
) -> Result<(), String> {
    // Récupérer le spot précédent
    let prev_spot = &state.spots[spot_index - 1];

    // Déterminer le joueur actuel (oop ou ip) en fonction du joueur précédent
    let player = if prev_spot.player == "oop" {
        "ip"
    } else {
        "oop"
    };

    // Analyser les actions à partir de la chaîne actions_str
    let actions: Vec<&str> = actions_str.split('/').collect();

    // Créer les actions pour le nouveau spot
    let player_actions: Vec<Action> = actions
        .iter()
        .enumerate()
        .map(|(i, action)| {
            let parts: Vec<&str> = action.split(':').collect();
            let name = parts[0].to_string();
            let amount = if parts.len() > 1 {
                parts[1].to_string()
            } else {
                "0".to_string()
            };

            Action {
                index: i,
                name,
                amount,
                is_selected: false,
                rate: -1.0,
            }
        })
        .collect();

    // Créer le nouveau spot de joueur
    let new_player_spot = Spot {
        spot_type: SpotType::Player,
        index: spot_index,
        player: player.to_string(),
        selected_index: -1,
        cards: Vec::new(),
        actions: player_actions,
        pot: prev_spot.pot,     // Utiliser le même pot que le spot précédent
        stack: prev_spot.stack, // Utiliser le même stack que le spot précédent
        equity_oop: 0.0,
        prev_player: Some(prev_spot.player.clone()),
    };

    // Remplacer tous les spots à partir de l'index actuel (comme dans le frontend)
    state.spots.truncate(spot_index);
    state.spots.push(new_player_spot);

    Ok(())
}

/// Fonction pour jouer une action sélectionnée (équivalent à play dans ResultNav.vue)
pub fn play(
    game: &mut PostFlopGame,
    state: &mut GameState,
    action_index: usize,
) -> Result<(), String> {
    let spot_index = state.selected_spot_index as usize;

    state.log_state("play()");
    println!("play() - action_index: {}", action_index);
    // Récupérer le spot actuel

    let spot: &mut Spot = match state.spots.get_mut(spot_index) {
        Some(s) => s,
        None => return Err(format!("Spot à l'index {} non trouvé", spot_index)),
    };

    // Vérifier que c'est bien un nœud de joueur
    if spot.spot_type != SpotType::Player {
        return Err(format!(
            "Le spot à l'index {} n'est pas un nœud joueur",
            spot_index
        ));
    }

    // Si une action est déjà sélectionnée, la désélectionner
    if spot.selected_index != -1 {
        if let Some(action) = spot.actions.get_mut(spot.selected_index as usize) {
            action.is_selected = false;
        }
    }

    // Sélectionner la nouvelle action
    if let Some(action) = spot.actions.get_mut(action_index) {
        action.is_selected = true;
    } else {
        return Err(format!("Action à l'index {} non trouvée", action_index));
    }

    // Mettre à jour l'index sélectionné
    spot.selected_index = action_index as i32;

    // Naviguer au spot suivant avec needSplice=true
    select_spot(game, state, spot_index + 1, true, false).map(|_| ())
}

/// Function to deal a card at the currently selected chance node
/// Based on deal() from ResultNav.vue
pub fn deal(
    game: &mut PostFlopGame,
    state: &mut GameState,
    card: usize,
) -> Result<SpecificResultData, String> {
    // Check if there's a selected chance node
    if state.selected_chance_index == -1 {
        return Err("Aucun nœud de chance sélectionné".to_string());
    }

    // Get a mutable reference to the chance spot
    let spot_index = state.selected_chance_index as usize;
    let spot = match state.spots.get_mut(spot_index) {
        Some(s) => {
            // Ensure it's a chance node
            if s.spot_type != SpotType::Chance {
                return Err("Le spot sélectionné n'est pas un nœud de chance".to_string());
            }
            s
        }
        None => return Err(format!("Spot à l'index {} non trouvé", spot_index)),
    };

    // Mark that we're in a dealing state
    state.is_dealing = true;

    // Deselect the previously selected card if any
    if spot.selected_index != -1 {
        if let Some(prev_card) = spot.cards.get_mut(spot.selected_index as usize) {
            prev_card.is_selected = false;
        }
    }

    // Select the new card
    if let Some(new_card) = spot.cards.get_mut(card) {
        // Check if the card is dead (unavailable)
        if new_card.is_dead {
            return Err(format!(
                "La carte à l'index {} est morte (indisponible)",
                card
            ));
        }
        new_card.is_selected = true;
    } else {
        return Err(format!("Carte à l'index {} non trouvée", card));
    }

    // Update the selected index
    spot.selected_index = card as i32;

    // Log the spot state after update
    state.log_spots("After card selection in deal()");

    // Call select_spot to update the game state with from_deal=true
    select_spot(game, state, state.selected_spot_index as usize, false, true)
}
