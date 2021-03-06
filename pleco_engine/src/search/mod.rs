//! The main searching function.

pub mod eval;

use std::cmp::{min,max};
use std::sync::atomic::{Ordering,AtomicBool};
use std::cell::UnsafeCell;

use rand;
use rand::Rng;

use pleco::{MoveList,Board,BitMove};
use pleco::core::*;
use pleco::tools::tt::*;
use pleco::core::score::*;
use pleco::tools::pleco_arc::Arc;

use MAX_PLY;
use TT_TABLE;

use threadpool::threadpool;
use time::time_management::TimeManager;
use time::uci_timer::*;
use threadpool::TIMER;
use sync::{GuardedBool,LockLatch};
use root_moves::RootMove;
use root_moves::root_moves_list::RootMoveList;
use tables::material::Material;
use tables::pawn_table::PawnTable;
use consts::*;

const THREAD_DIST: usize = 20;

//                                      1  2  3  4  5  6  7  8  9 10 11 12 13 14 15 16 17 18 19 20
static SKIP_SIZE: [u16; THREAD_DIST] = [1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 3, 3, 4, 4, 4, 4, 4, 4, 4, 4];
static START_PLY: [u16; THREAD_DIST] = [0, 1, 0, 1, 2, 3, 0, 1, 2, 3, 4, 5, 0, 1, 2, 3, 4, 5, 6, 7];

pub struct ThreadStack {
    pv: BitMove,
    ply: u16,
}

pub struct Searcher {
    // Synchronization primitives
    pub id: usize,
    pub kill: AtomicBool,
    pub searching: Arc<GuardedBool>,
    pub cond: Arc<LockLatch>,

    // search data
    pub depth_completed: u16,
    pub limit: Limits,
    pub board: Board,
    pub time_man: &'static TimeManager,
    pub tt: &'static TranspositionTable,
    pub pawns: PawnTable,
    pub material: Material,
    pub root_moves: UnsafeCell<RootMoveList>,

    // MainThread Information
    pub previous_score: Value,

}

unsafe impl Send for Searcher {}
unsafe impl Sync for Searcher {}

impl Searcher {
    pub fn new(id: usize, cond: Arc<LockLatch>) -> Self {
        Searcher {
            id,
            kill: AtomicBool::new(false),
            searching: Arc::new(GuardedBool::new(true)),
            cond,
            depth_completed: 0,
            limit: Limits::blank(),
            board: Board::default(),
            time_man: &TIMER,
            tt: &TT_TABLE,
            pawns: PawnTable::new(16384),
            material: Material::new(8192),
            root_moves: UnsafeCell::new(RootMoveList::new()),
            previous_score: 0
        }
    }

    pub fn idle_loop(&mut self) {
        self.searching.set(false);
        loop {
            self.cond.wait();
            if self.kill.load(Ordering::SeqCst) {
                return;
            }
            self.go();
        }
    }

    fn go(&mut self) {
        self.searching.set(true);
        if self.main_thread() {
            self.main_thread_go();
        } else {
            self.search_root();
        }
        self.searching.set(false);
    }

    fn main_thread_go(&mut self) {
        // set the global limit
        if let Some(timer) = self.limit.use_time_management() {
            TIMER.init(self.limit.start.clone(), &timer, self.board.turn(), self.board.moves_played());
        }

        // Start each of the threads!
        threadpool().thread_cond.set();

        // Search ourselves
        self.search_root();

        threadpool().thread_cond.lock();
        threadpool().set_stop(true);
        threadpool().wait_for_non_main();

        let mut best_move = self.root_moves().first().bit_move;
        let mut best_score = self.root_moves().first().score;
        if let LimitsType::Depth(_) = self.limit.limits_type  {
            let mut best_thread: &Searcher = &self;
            threadpool().threads.iter().map(|u| unsafe {&**u.get()}).for_each(|th| {
                let depth_diff = th.depth_completed as i32 - best_thread.depth_completed as i32;
                let score_diff = th.root_moves().first().score - best_thread.root_moves()[0].score;
                if score_diff > 0 && depth_diff >= 0 {
                    best_thread = th;
                }
            });
            best_move =  best_thread.root_moves().first().bit_move;
            best_score = best_thread.root_moves().first().score;
        }

        self.previous_score = best_score;

        if self.use_stdout() && best_move != self.root_moves().first().bit_move {
            println!("info id 0 pv {}",self.root_moves().first().bit_move);
        }

        if self.use_stdout() {
            println!("bestmove {}", best_move.to_string());
        }
    }


    fn search_root(&mut self) {
        assert_eq!(self.board.depth(), 0);

        if self.stop() {
            return;
        }

        if self.use_stdout() {
            println!("info id {} start", self.id);
        }

        let max_depth = if self.main_thread() {
            if let LimitsType::Depth(d) = self.limit.limits_type {
                d
            } else {
                MAX_PLY
            }
        } else {
            MAX_PLY
        };

        let start_ply: u16 = START_PLY[self.id % THREAD_DIST];
        let skip_size: u16 = SKIP_SIZE[self.id % THREAD_DIST];
        let mut depth: u16 = start_ply;

        let mut delta: i32 = NEG_INFINITE as i32;
        #[allow(unused_assignments)]
        let mut best_value: i32 = NEG_INFINITE as i32;
        let mut alpha: i32 = NEG_INFINITE as i32;
        let mut beta: i32 = INFINITE as i32;

        let mut time_reduction: f64 = 1.0;
        let mut last_best_move: BitMove = BitMove::null();
        let mut best_move_stability: u32 = 0;

        self.shuffle();


        'iterative_deepening: while (!self.stop() || !self.main_thread()) && depth < max_depth {
            self.root_moves().rollback();

            let prev_best_score = self.root_moves()[0].prev_score;

            if depth >= 5 {
                delta = 18;
                alpha = max(prev_best_score - delta, NEG_INFINITE as i32);
                beta = min(prev_best_score + delta, INFINITE as i32);
            }

            'aspiration_window: loop {

                best_value = self.search::<PV>(alpha, beta, depth) as i32;
                self.root_moves().sort();

                if self.stop() {
                    break 'aspiration_window;
                }

                if best_value <= alpha {
                    alpha = max(best_value - delta, NEG_INFINITE as i32);
                } else if best_value >= beta {
                    beta = min(best_value + delta, INFINITE as i32);
                } else {
                    break 'aspiration_window;
                }
                delta += (delta / 4) + 5;

                assert!(alpha >= NEG_INFINITE as i32);
                assert!(beta <= INFINITE as i32);
            }

            self.root_moves().sort();
            if self.use_stdout() && self.main_thread() {
                println!("info depth {} score {} pv {}",
                         depth,
                         best_value,
                         self.root_moves().first().bit_move.to_string());
            }
            if !self.stop() {
                self.depth_completed = depth;
            }
            depth += skip_size;

            if !self.main_thread() {
                continue;
            }

            let best_move = unsafe { self.root_moves().get_unchecked(0).bit_move};
            if best_move != last_best_move {
                time_reduction = 1.0;
                best_move_stability = 0;
            } else {
                time_reduction *= 0.91;
                best_move_stability += 1;
            }

            last_best_move = best_move;

            // check for time
            if let Some(_) = self.limit.use_time_management() {
                if !self.stop() {
                    let ideal = TIMER.ideal_time();
                    let elapsed = TIMER.elapsed();
                    let stability: f64 = f64::powi(0.92, best_move_stability as i32);
                    let new_ideal = (ideal as f64 * stability * time_reduction) as i64;
                    println!("ideal: {}, new_ideal: {}, elapsed: {}", ideal, new_ideal, elapsed);
                    if self.root_moves().len() == 1 || TIMER.elapsed() >= new_ideal {
                        break 'iterative_deepening;
                    }
                }
            }

        }
    }

    fn search<N: PVNode>(&mut self, mut alpha: i32, beta: i32, max_depth: u16) -> i32 {
        let is_pv: bool = N::is_pv();
        let at_root: bool = self.board.depth() == 0;
        let zob: u64 = self.board.zobrist();
        let (tt_hit, tt_entry): (bool, &mut Entry) = TT_TABLE.probe(zob);
        let tt_value: Value = if tt_hit {tt_entry.score as i32} else {0};
        let in_check: bool = self.board.in_check();
        let ply: u16 = self.board.depth();

        let mut best_move = BitMove::null();

        let mut value: Value = NEG_INFINITE;
        let mut best_value: Value = NEG_INFINITE;
        let mut moves_played = 0;

        let mut pos_eval: i32 = 0;

        if self.main_thread() {
            self.check_time();
        }

        if ply >= max_depth || self.stop() {
            return self.eval();
        }

        let plys_to_zero = max_depth - ply;

        if !at_root {
            if alpha >= beta {
                return alpha
            }
        }

        if !is_pv
            && tt_hit
            && tt_entry.depth as u16 >= plys_to_zero
            && tt_entry.eval != 0
            && tt_value != 0
            && pos_eval != 0
            && correct_bound_eq(tt_value, beta, tt_entry.node_type()) {
            return tt_value;
        }

        if in_check {
            pos_eval = 0;
        } else {
            if tt_hit {
                if tt_entry.eval == 0 {
                    pos_eval = self.eval();
                }
                if tt_value != 0 && correct_bound(tt_value, pos_eval, tt_entry.node_type()) {
                    pos_eval = tt_value;
                }
            } else {
                pos_eval = self.eval();
                tt_entry.place(zob, BitMove::null(), 0, pos_eval as i16, 0, NodeBound::NoBound);
            }
        }

        if !in_check {
            if ply > 3
                && ply < 7
                && pos_eval - futility_margin(ply) >= beta && pos_eval < 10000 {
                return pos_eval;
            }
        }

        #[allow(unused_mut)]
        let mut moves: MoveList = if at_root {
            self.root_moves().iter().map(|r| r.bit_move).collect()
        } else {
            self.board.generate_pseudolegal_moves()
        };

        if moves.is_empty() {
            if self.board.in_check() {
                return MATE as i32 - (ply as i32);
            } else {
                return DRAW as i32;
            }
        }

        if !at_root {
            mvv_lva_sort(&mut moves, &self.board);
        }


        for (i, mov) in moves.iter().enumerate() {
            if at_root || self.board.legal_move(*mov) {
                moves_played += 1;
                let gives_check: bool = self.board.gives_check(*mov);
                self.board.apply_unknown_move(*mov, gives_check);
                self.tt.prefetch(self.board.zobrist());
                let do_full_depth: bool = if max_depth >= 3 && moves_played > 1 && ply >= 2 {
                    if in_check || gives_check {
                        value = -self.search::<NonPV>(-(alpha+1), -alpha, max_depth - 1);
                    } else {
                        value = -self.search::<NonPV>(-(alpha+1), -alpha, max_depth - 2);
                    }
                    value > alpha
                } else {
                    !is_pv || moves_played > 1
                };
                if do_full_depth {
                    value = -self.search::<NonPV>(-(alpha+1), -alpha, max_depth);
                }
                if is_pv && (moves_played == 1 || (value > alpha && (at_root || value < beta))) {
                    value = -self.search::<PV>(-beta, -alpha, max_depth);
                }
                self.board.undo_move();
                assert!(value > NEG_INFINITE);
                assert!(value < INFINITE );
                if self.stop() {
                    return 0;
                }
                if at_root {
                    let rm: &mut RootMove = unsafe { self.root_moves().get_unchecked_mut(i) };

                    if moves_played == 1 || value > alpha {
                        rm.depth_reached = max_depth;
                        rm.score = value;

                    } else {
                        rm.score = NEG_INFINITE;
                    }
                }

                if value > best_value {
                    best_value = value;

                    if value > alpha {
                        best_move = *mov;
                        if is_pv && value < beta {
                            alpha = value;
                        } else {
                            break;
                        }
                    }
                }
            }
        }

        if moves_played == 0 {
            if self.board.in_check() {
                return MATE as i32 - (ply as i32);
            } else {
                return DRAW as i32;
            }
        }

        let node_bound = if best_value as i32 >= beta {NodeBound::LowerBound}
            else if is_pv && !best_move.is_null() {NodeBound::Exact}
                else {NodeBound::UpperBound};


        tt_entry.place(zob, best_move, best_value as i16, pos_eval as i16, plys_to_zero as u8, node_bound);

        best_value
    }

    // TODO: Qscience search

    pub fn eval(&mut self) -> Value {
        let pawns = &mut self.pawns;
        let material = &mut self.material;
        eval::Evaluation::evaluate(&self.board, pawns, material)
    }

    #[inline(always)]
    fn main_thread(&self) -> bool {
        self.id == 0
    }

    #[inline(always)]
    fn stop(&self) -> bool {
        threadpool().stop.load(Ordering::Relaxed)
    }

    fn check_time(&mut self) {
        if self.limit.use_time_management().is_some()
            && TIMER.elapsed() >= TIMER.maximum_time() {
            threadpool().set_stop(true);
        } else if let Some(time) = self.limit.use_movetime() {
            if self.limit.elapsed() >= time as i64 {
                threadpool().set_stop(true);
            }
        }
    }

    #[inline(always)]
    pub fn print_startup(&self) {
        if self.use_stdout() {
            println!("info id {} start", self.id);
        }
    }

    #[inline(always)]
    pub fn use_stdout(&self) -> bool {
        USE_STDOUT.load(Ordering::Relaxed)
    }

    pub fn shuffle(&mut self) {
        if self.id == 0 || self.id >= 20 {
            self.rm_mvv_laa_sort();
        } else {
            rand::thread_rng().shuffle(self.root_moves().as_mut());
        }
    }

    #[inline]
    pub fn root_moves(&self) -> &mut RootMoveList {
        unsafe {
            &mut *self.root_moves.get()
        }
    }

    #[inline]
    fn rm_mvv_laa_sort(&mut self) {
        let board = &self.board;
        self.root_moves().sort_by_key(|root_move| {
            let a = root_move.bit_move;
            let piece = board.piece_at_sq((a).get_src()).unwrap();

            if a.is_capture() {
                piece.value() - board.captured_piece(a).unwrap().value()
            } else if a.is_castle() {
                1
            } else if piece == PieceType::P {
                if a.is_double_push().0 {
                    2
                } else {
                    3
                }
            } else {
                4
            }
        });
    }
}


impl Drop for Searcher {
    fn drop(&mut self) {
        self.searching.set(false);
    }
}

fn mvv_lva_sort(moves: &mut MoveList, board: &Board) {
    moves.sort_by_key(|a| {
        let piece = board.piece_at_sq((*a).get_src()).unwrap();

        if a.is_capture() {
            piece.value() - board.captured_piece(*a).unwrap().value()
        } else if a.is_castle() {
            1
        } else if piece == PieceType::P {
            if a.is_double_push().0 {
                2
            } else {
                3
            }
        } else {
            4
        }
    })
}

fn correct_bound_eq(tt_value: i32, beta: i32, bound: NodeBound) -> bool {
    if tt_value as i32 >= beta {
        bound as u8 & NodeBound::LowerBound as u8 != 0
    } else {
        bound as u8 & NodeBound::UpperBound as u8 != 0
    }
}

fn correct_bound(tt_value: i32, val: i32, bound: NodeBound) -> bool {
    if tt_value as i32 > val {
        bound as u8 & NodeBound::LowerBound as u8 != 0
    } else {
        bound as u8 & NodeBound::UpperBound as u8 != 0
    }
}

#[inline]
fn futility_margin(depth: u16) -> i32 {
    depth as i32 * 150
}