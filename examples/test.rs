use postflop_solver::*;

fn main() {
    println!("=== TEST DE VÉRIFICATION DU POT ===");

    // Configuration du jeu avec les mêmes paramètres que run.rs
    let oop_range = "66+,A8s+,A5s-A4s,AJo+,K9s+,KQo,QTs+,JTs,96s+,85s+,75s+,65s,54s";
    let ip_range = "QQ-22,AQs-A2s,ATo+,K5s+,KJo+,Q8s+,J8s+,T7s+,96s+,86s+,75s+,64s+,53s+";

    let card_config = CardConfig {
        range: [oop_range.parse().unwrap(), ip_range.parse().unwrap()],
        flop: flop_from_str("Td5d3h").unwrap(),
        turn: NOT_DEALT,
        river: NOT_DEALT,
    };

    // Configuration simple avec un pot de départ de 20bb et un stack de 100bb
    let tree_config = TreeConfig {
        initial_state: BoardState::Flop,
        starting_pot: 20, // Pot initial de 20bb
        effective_stack: 100,
        rake_rate: 0.0,
        rake_cap: 0.0,
        flop_bet_sizes: [
            BetSizeOptions::try_from(("50%", "60%")).unwrap(),
            BetSizeOptions::try_from(("50%", "60%")).unwrap(),
        ],
        turn_bet_sizes: [
            BetSizeOptions::try_from(("50%", "60%")).unwrap(),
            BetSizeOptions::try_from(("50%", "60%")).unwrap(),
        ],
        river_bet_sizes: [
            BetSizeOptions::try_from(("50%", "60%")).unwrap(),
            BetSizeOptions::try_from(("50%", "60%")).unwrap(),
        ],
        turn_donk_sizes: None,
        river_donk_sizes: None,
        add_allin_threshold: 1.5,
        force_allin_threshold: 0.20,
        merging_threshold: 0.1,
    };

    // Construction du jeu
    let action_tree = ActionTree::new(tree_config.clone()).unwrap();
    let mut game = PostFlopGame::with_config(card_config, action_tree).unwrap();

    println!("Allocation de la mémoire...");
    game.allocate_memory(false);

    println!("Résolution rapide pour initialiser les structures de données...");
    // Effectuer 10 itérations en utilisant la fonction importée
    for i in 0..10 {
        solve_step(&mut game, i);
    }

    // Vérifier le pot initial
    let total_bet = game.total_bet_amount();
    let pot_size = calculate_pot_size(&game);
    println!("Pot initial: {:.2} bb", pot_size);
    assert_eq!(pot_size, 20.0, "Le pot initial devrait être de 20bb");

    // OOP est le premier à jouer au flop
    let player = game.current_player();
    assert_eq!(
        player, 0,
        "Le joueur OOP (0) devrait être le premier à jouer"
    );

    // Afficher les actions disponibles
    let actions = game.available_actions();
    println!("Actions disponibles pour OOP:");
    for (i, action) in actions.iter().enumerate() {
        println!("  {}: {:?}", i, action);
    }

    // Jouer la mise (BET)
    let bet_idx = actions
        .iter()
        .position(|a| {
            if let Action::Bet(amount) = a {
                *amount == 10 // Chercher une mise de 10bb (50% du pot)
            } else {
                false
            }
        })
        .expect("L'action BET 10 devrait être disponible");

    println!("\nOOP joue BET 10bb");
    game.play(bet_idx);

    // Vérifier le pot après la mise
    let total_bet = game.total_bet_amount();
    let pot_size = calculate_pot_size(&game);
    println!("Pot après mise de OOP: {:.2} bb", pot_size);
    println!(
        "Total bet amounts: OOP={}, IP={}",
        total_bet[0], total_bet[1]
    );
    assert_eq!(
        pot_size, 30.0,
        "Le pot devrait être de 30bb après la mise de 10bb"
    );
    assert_eq!(total_bet[0], 10, "OOP devrait avoir misé 10bb");
    assert_eq!(total_bet[1], 0, "IP ne devrait pas encore avoir misé");

    // Maintenant IP doit agir
    let player = game.current_player();
    assert_eq!(player, 1, "Le joueur IP (1) devrait jouer maintenant");

    // Afficher les actions disponibles pour IP
    let actions = game.available_actions();
    println!("\nActions disponibles pour IP:");
    for (i, action) in actions.iter().enumerate() {
        println!("  {}: {:?}", i, action);
    }

    // IP décide de suivre (CALL)
    let call_idx = actions
        .iter()
        .position(|a| matches!(a, Action::Call))
        .expect("L'action CALL devrait être disponible");

    println!("\nIP joue CALL");
    game.play(call_idx);

    // Vérifier le pot après le call
    let total_bet = game.total_bet_amount();
    let pot_size = calculate_pot_size(&game);
    println!("Pot après call de IP: {:.2} bb", pot_size);
    println!(
        "Total bet amounts: OOP={}, IP={}",
        total_bet[0], total_bet[1]
    );
    assert_eq!(pot_size, 40.0, "Le pot devrait être de 40bb après le call");
    assert_eq!(total_bet[0], 10, "OOP devrait toujours avoir misé 10bb");
    assert_eq!(
        total_bet[1], 10,
        "IP devrait maintenant avoir misé 10bb aussi"
    );

    // Test de distribution d'une carte turn
    if game.is_chance_node() {
        println!("\nDistribution de la carte turn");
        let actions = game.available_actions();
        if !actions.is_empty() {
            game.play(0); // Distribuer la première carte disponible

            // Vérifier que le pot reste inchangé après la distribution
            let total_bet = game.total_bet_amount();
            let pot_size = calculate_pot_size(&game);
            println!("Pot après distribution turn: {:.2} bb", pot_size);
            assert_eq!(
                pot_size, 40.0,
                "Le pot ne devrait pas changer après la distribution d'une carte"
            );

            // OOP devrait à nouveau être le premier à jouer
            let player = game.current_player();
            assert_eq!(player, 0, "OOP devrait jouer en premier au turn");

            // Supposons que OOP check
            let actions = game.available_actions();
            let check_idx = actions
                .iter()
                .position(|a| matches!(a, Action::Check))
                .expect("L'action CHECK devrait être disponible");

            println!("\nOOP joue CHECK");
            game.play(check_idx);

            // Vérifier que le pot reste inchangé après le check
            let total_bet = game.total_bet_amount();
            let pot_size = calculate_pot_size(&game);
            println!("Pot après check de OOP: {:.2} bb", pot_size);
            assert_eq!(
                pot_size, 40.0,
                "Le pot ne devrait pas changer après un check"
            );
        }
    }

    println!("\n=== TESTS TERMINÉS AVEC SUCCÈS ===");
}

// Fonction pour calculer le pot correctement
fn calculate_pot_size(game: &PostFlopGame) -> f32 {
    let total_bet = game.total_bet_amount();
    let pot_base = game.tree_config().starting_pot as f32;
    let common_bet = total_bet[0].min(total_bet[1]) as f32;
    let extra_bet = (total_bet[0].max(total_bet[1]) - total_bet[0].min(total_bet[1])) as f32;
    pot_base + 2.0 * common_bet + extra_bet
}
