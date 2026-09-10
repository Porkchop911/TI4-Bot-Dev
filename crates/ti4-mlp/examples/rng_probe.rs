//! What a fresh seed-0 stream rolls, which is what every agenda-effect die and every harness game
//! starts from.
fn main() {
    let dice = ti4_engine::rng::domain::DICE;
    let mut fresh = ti4_engine::rng::GameRng::new(0);
    println!("fresh GameRng::new(0), domain {dice:?}, first d10: {}", fresh.die(dice, 10));
    let mut game = ti4_engine::rng::GameRng::new(0);
    let faces: Vec<u32> = (0..12).map(|_| game.die(dice, 10)).collect();
    println!("first 12 combat dice of every harness game: {faces:?}");
    for seed in [910_001_000_u64, 910_001_001, 910_001_002] {
        let mut rng = ti4_engine::rng::GameRng::new(seed);
        let faces: Vec<u32> = (0..12).map(|_| rng.die(dice, 10)).collect();
        println!("if seeded {seed}: {faces:?}");
    }
}
