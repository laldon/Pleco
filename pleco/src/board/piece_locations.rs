//! Contains the `PieceLocations` structure that maps from squares of a board to a player / piece at that square.
//!
//! This is useful mainly for the [`Board`] to use internally for fast square lookups.
//!
//! [`Board`]: ../struct.Board.html
//! [`PieceLocations`]: struct.PieceLocations.html

use core::*;
use std::mem;
use core::sq::SQ;
use core::masks::{PLAYER_CNT, PIECE_TYPE_CNT};
use super::FenBuildError;

/// Struct to allow fast lookups for any square. Given a square, allows for determining if there
/// is a piece currently there, and if so, allows for determining it's color and type of piece.
///
/// Piece Locations is a BLIND structure, Providing a function of  |sq| -> |Piece AND/OR Player|
/// The reverse cannot be done Looking up squares from a piece / player.
pub struct PieceLocations {
    // Pieces are represented by the following bit_patterns:
    // x000 -> Pawn (P)
    // x001 -> Knight(N)
    // x010 -> Bishop (B)
    // x011 -> Rook(R)
    // x100 -> Queen(Q)
    // x101 -> King (K)
    // x110 -> ??? Undefined ??
    // x111 -> None
    // 0xxx -> White Piece
    // 1xxx -> Black Piece

    // array of u8's, with standard ordering mapping index to square
    data: [u8; 64],
}



impl PieceLocations {
    /// Constructs a new `PieceLocations` with a default of no pieces on the board.
    pub const fn blank() -> PieceLocations {
        PieceLocations { data: [0b0111; 64] }
    }

    /// Constructs a new `PieceLocations` with the memory at a default of Zeros.
    ///
    /// This function is unsafe as Zeros represent Pawns, and therefore care mus be taken
    /// to iterate through every square and ensure the correct piece or lack of piece
    /// is placed.
    pub const fn default() -> PieceLocations {
        PieceLocations { data: [0; 64] }
    }

    /// Places a given piece for a given player at a certain square.
    ///
    /// # Panics
    ///
    /// Panics if Square is of index higher than 63.
    #[inline]
    pub fn place(&mut self, square: SQ, player: Player, piece: PieceType) {
        assert!(square.is_okay());
        self.data[square.0 as usize] = self.create_sq(player, piece);
    }

    /// Removes a Square.
    ///
    /// # Panics
    ///
    /// Panics if Square is of index higher than 63.
    #[inline]
    pub fn remove(&mut self, square: SQ) {
        assert!(square.is_okay());
        self.data[square.0 as usize] = 0b0111
    }

    /// Returns the Piece at a `SQ`, Or None if the square is empty.
    ///
    /// # Panics
    ///
    /// Panics if square is of index higher than 63.
    #[inline]
    pub fn piece_at(&self, square: SQ) -> Option<PieceType> {
        debug_assert!(square.is_okay());
        let byte: u8 = self.data[square.0 as usize] & 0b0111;
        match byte {
            0b0000 => Some(PieceType::P),
            0b0001 => Some(PieceType::N),
            0b0010 => Some(PieceType::B),
            0b0011 => Some(PieceType::R),
            0b0100 => Some(PieceType::Q),
            0b0101 => Some(PieceType::K),
            0b0110 => unreachable!(), // Undefined
            0b0111 => None,
            _ => unreachable!(),
        }
    }

    /// Returns the Piece at a `SQ` for a given player.
    ///
    /// If there is no piece at that square, or there is a piece of another player at that square,
    /// returns None.
    ///
    /// # Panics
    ///
    /// Panics if Square is of index higher than 63.
    #[inline]
    pub fn piece_at_for_player(&self, square: SQ, player: Player) -> Option<PieceType> {
        let op = self.player_piece_at(square);
        if op.is_some() {
            let p = op.unwrap();
            if p.0 == player {
                Some(p.1)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Returns the `Player` (if any) is occupying a `SQ`.
    ///
    /// # Panics
    ///
    /// Panics if Square is of index higher than 63.
    #[inline]
    pub fn player_at(&self, square: SQ) -> Option<Player> {
        let byte: u8 = self.data[square.0 as usize];
        if byte == 0b0111 || byte == 0b1111 {
            return None;
        }
        if byte < 8 {
            Some(Player::White)
        } else {
            Some(Player::Black)
        }
    }

    /// Returns a Tuple of `(Player,Piece)` of the player and associated piece at a
    /// given square. Returns None if the square is unoccupied.
    ///
    /// # Panics
    ///
    /// Panics if Square is of index higher than 63.
    #[inline]
    pub fn player_piece_at(&self, square: SQ) -> Option<(Player, PieceType)> {
        let byte: u8 = self.data[square.0 as usize];
        match byte {
            0b0000 => Some((Player::White, PieceType::P)),
            0b0001 => Some((Player::White, PieceType::N)),
            0b0010 => Some((Player::White, PieceType::B)),
            0b0011 => Some((Player::White, PieceType::R)),
            0b0100 => Some((Player::White, PieceType::Q)),
            0b0101 => Some((Player::White, PieceType::K)),
            0b0110 => unreachable!(), // Undefined
            0b0111 | 0b1111 => None,
            0b1000 => Some((Player::Black, PieceType::P)),
            0b1001 => Some((Player::Black, PieceType::N)),
            0b1010 => Some((Player::Black, PieceType::B)),
            0b1011 => Some((Player::Black, PieceType::R)),
            0b1100 => Some((Player::Black, PieceType::Q)),
            0b1101 => Some((Player::Black, PieceType::K)),
            0b1110 => unreachable!(), // Undefined
            _ => unreachable!(),
        }
    }

    /// Returns if there is a `SQ` is occupied.
    #[inline]
    pub fn at_square(&self, square: SQ) -> bool {
        assert!(square.is_okay());
        let byte: u8 = self.data[square.0 as usize];
        byte != 0b0111 && byte != 0b1111
    }

    /// Returns the first square (if any) that a piece / player is at.
    #[inline]
    pub fn first_square(&self, piece: PieceType, player: Player) -> Option<SQ> {
        let target = self.create_sq(player, piece);
        for x in 0..64 {
            if target == self.data[x as usize] {
                return Some(SQ(x));
            }
        }
        None
    }

    /// Returns if the Board contains a particular piece / player.
    #[inline]
    pub fn contains(&self, piece: PieceType, player: Player) -> bool {
        self.first_square(piece,player).is_some()
    }



    /// Generates a `PieceLocations` from a partial fen. A partial fen is defined as the first part of a
    /// fen, where the piece positions are available.
    pub fn from_partial_fen(ranks: &[&str]) -> Result<(PieceLocations,[[u8; PIECE_TYPE_CNT]; PLAYER_CNT]), FenBuildError> {
        let mut loc = PieceLocations::blank();
        let mut piece_cnt: [[u8; PIECE_TYPE_CNT]; PLAYER_CNT] = [[0; PIECE_TYPE_CNT]; PLAYER_CNT];
        for (i, rank) in ranks.iter().enumerate() {
            let min_sq = (7 - i) * 8;
            let max_sq = min_sq + 7;
            let mut idx = min_sq;
            for ch in rank.chars() {
                if idx < min_sq {
                    return Err(FenBuildError::SquareSmallerRank{rank: i, square: SQ(idx as u8).to_string()})
                } else if idx > max_sq {
                    return Err(FenBuildError::SquareLargerRank{rank: i, square: SQ(idx as u8).to_string()})
                }

                let dig = ch.to_digit(10);
                if let Some(digit) = dig {
                    idx += digit as usize;
                } else {
                    // if no space, then there is a piece here
                    let piece = match ch {
                        'p' | 'P' => PieceType::P,
                        'n' | 'N' => PieceType::N,
                        'b' | 'B' => PieceType::B,
                        'r' | 'R' => PieceType::R,
                        'q' | 'Q' => PieceType::Q,
                        'k' | 'K' => PieceType::K,
                        _ => {return Err(FenBuildError::UnrecognizedPiece{piece: ch})},
                    };
                    let player = if ch.is_lowercase() {
                        Player::Black
                    } else {
                        Player::White
                    };
                    loc.place(SQ(idx as u8), player, piece);
                    piece_cnt[player as usize][piece as usize] += 1;
                    idx += 1;
                }
            }
        }
        Ok((loc,piece_cnt))
    }


    /// Helper method to return the bit representation of a given piece and player.
    #[inline]
    fn create_sq(&self, player: Player, piece: PieceType) -> u8 {
        let mut loc: u8 = match piece {
            PieceType::P => 0b0000,
            PieceType::N => 0b0001,
            PieceType::B => 0b0010,
            PieceType::R => 0b0011,
            PieceType::Q => 0b0100,
            PieceType::K => 0b0101,
        };
        if player == Player::Black {
            loc |= 0b1000;
        }
        loc
    }
}

impl Clone for PieceLocations {
    // Need to use transmute copy as [_;64] does not automatically implement Clone.
    fn clone(&self) -> PieceLocations {
        unsafe { mem::transmute_copy(&self.data) }
    }
}

impl PartialEq for PieceLocations {
    fn eq(&self, other: &PieceLocations) -> bool {
        for sq in 0..64 {
            if self.data[sq] != other.data[sq] {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::PieceLocations;
    use {SQ, PieceType, Player};

    #[test]
    fn piece_loc_blank() {
        let mut l = PieceLocations::blank();
        for s in 0..64 {
            assert!(l.piece_at(SQ(s)).is_none());
        }
        l.place(SQ(3), Player::White, PieceType::P);
        assert_eq!(l.piece_at(SQ(3)).unwrap(), PieceType::P);
        assert_eq!(l.player_at(SQ(3)).unwrap(), Player::White);
        assert_eq!(l.player_piece_at(SQ(3)).unwrap(),(Player::White, PieceType::P));
        assert!(l.at_square(SQ(3)));
        for s in 0..64 {
            if s != 3 {
                assert!(l.piece_at(SQ(s)).is_none());
            }
        }
        l.place(SQ(3), Player::Black, PieceType::K);
        assert_eq!(l.piece_at(SQ(3)).unwrap(), PieceType::K);
        assert_eq!(l.player_at(SQ(3)).unwrap(), Player::Black);
        assert_eq!(l.player_piece_at(SQ(3)).unwrap(),(Player::Black, PieceType::K));
        assert!(l.at_square(SQ(3)));
        assert!(l.contains(PieceType::K, Player::Black));
        for s in 0..64 {
            if s != 3 {
                assert!(l.piece_at(SQ(s)).is_none());
            }
        }
        l.remove(SQ(3));
        for s in 0..64 {
            assert!(l.piece_at(SQ(s)).is_none());
        }
        l.remove(SQ(3));
        for s in 0..64 {
            assert!(l.piece_at(SQ(s)).is_none());
        }
        let c = l.clone();
        assert!(c == l);
    }
}