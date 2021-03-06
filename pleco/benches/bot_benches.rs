#![feature(test)]
extern crate pleco;
extern crate test;
extern crate rand;

#[macro_use]
extern crate lazy_static;

use pleco::Board;
use pleco::bot_prelude::*;


use test::{black_box, Bencher};

lazy_static! {
    pub static ref RAND_BOARDS: Vec<Board> = {
        let mut vec = Vec::new();
        vec.push(Board::default());
        vec
    };
}


#[bench]
fn _4_ply_minimax(b: &mut Bencher) {
    b.iter(|| {
        for board in RAND_BOARDS.iter() {
            black_box(MiniMaxSearcher::best_move_depth(board.shallow_clone(), 4));
        }
    })
}

#[bench]
fn _4_ply_parallel_minimax(b: &mut Bencher) {
    b.iter(|| {
        for board in RAND_BOARDS.iter() {
            black_box(ParallelMiniMaxSearcher::best_move_depth(board.shallow_clone(), 4));
        }
    })
}

#[bench]
fn _4_ply_alpha_beta(b: &mut Bencher) {
    b.iter(|| {
        for board in RAND_BOARDS.iter() {
            black_box(AlphaBetaSearcher::best_move_depth(board.shallow_clone(), 4));
        }
    })
}

#[bench]
fn _4_ply_jamboree(b: &mut Bencher) {
    b.iter(|| {
        for board in RAND_BOARDS.iter() {
            black_box(JamboreeSearcher::best_move_depth(board.shallow_clone(),4));
        }
    })
}