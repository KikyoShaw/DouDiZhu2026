use super::cards::rank;
use super::combo::{candidates, resolve, Combo, ComboKind};

#[derive(Clone, Debug)]
pub struct Knowledge {
    pub hand: Vec<u8>,
    pub seat: usize,
    pub landlord: Option<usize>,
    pub counts: [usize; 3],
    pub previous: Option<Combo>,
    pub previous_seat: Option<usize>,
    pub played: Vec<u8>,
}

type RankCounts = [u8; 18];

fn valid_hand(hand: &[u8]) -> bool {
    if hand.is_empty() || hand.len() > 20 {
        return false;
    }
    let mut seen = 0u64;
    for &id in hand {
        if id >= 54 {
            return false;
        }
        let bit = 1u64 << id;
        if seen & bit != 0 {
            return false;
        }
        seen |= bit;
    }
    true
}

fn rank_counts(ids: &[u8]) -> RankCounts {
    let mut result: RankCounts = [0; 18];
    let mut seen = 0u64;
    for &id in ids {
        if id < 54 && seen & (1u64 << id) == 0 {
            seen |= 1u64 << id;
            result[rank(id) as usize] += 1;
        }
    }
    result
}

fn is_opponent(knowledge: &Knowledge, seat: usize) -> bool {
    if seat >= 3 || seat == knowledge.seat {
        return false;
    }
    match knowledge.landlord.filter(|&landlord| landlord < 3) {
        Some(landlord) if knowledge.seat == landlord => true,
        Some(landlord) => seat == landlord,
        None => true,
    }
}

fn teammate_played(knowledge: &Knowledge) -> bool {
    match (knowledge.landlord, knowledge.previous_seat) {
        (Some(landlord), Some(previous)) => {
            landlord < 3
                && previous < 3
                && previous != knowledge.seat
                && knowledge.seat != landlord
                && previous != landlord
        }
        _ => false,
    }
}

fn unseen_cards(knowledge: &Knowledge) -> RankCounts {
    let mut unseen: RankCounts = [0; 18];
    for count in &mut unseen[3..=15] {
        *count = 4;
    }
    unseen[16] = 1;
    unseen[17] = 1;
    let mut seen = 0u64;
    for &id in knowledge
        .hand
        .iter()
        .chain(knowledge.played.iter())
        .chain(knowledge.previous.iter().flat_map(|combo| combo.cards.iter()))
    {
        if id < 54 && seen & (1u64 << id) == 0 {
            seen |= 1u64 << id;
            let r = rank(id) as usize;
            unseen[r] = unseen[r].saturating_sub(1);
        }
    }
    unseen
}

// The unseen pool is public information, not an opponent's actual hand. A
// possible response is a conservative threat estimate, never a hidden-card read.
fn could_cover(play: &Combo, unseen: &RankCounts, hand_size: usize) -> bool {
    if play.kind == ComboKind::Rocket {
        return false;
    }
    if hand_size >= 2 && unseen[16] == 1 && unseen[17] == 1 {
        return true;
    }
    if hand_size >= 4
        && (3..=15).any(|r| {
            unseen[r] == 4 && (play.kind != ComboKind::Bomb || r as u8 > play.main_rank)
        })
    {
        return true;
    }
    if play.kind == ComboKind::Bomb || hand_size < play.length {
        return false;
    }
    match play.kind {
        ComboKind::Single | ComboKind::Pair | ComboKind::Triple => {
            let unit = match play.kind {
                ComboKind::Single => 1,
                ComboKind::Pair => 2,
                _ => 3,
            };
            (3..=17).any(|r| r as u8 > play.main_rank && unseen[r] >= unit)
        }
        ComboKind::TripleSingle | ComboKind::TriplePair => {
            let wing = if play.kind == ComboKind::TriplePair { 2 } else { 1 };
            (3..=15).any(|r| {
                r as u8 > play.main_rank
                    && unseen[r] >= 3
                    && (3..=17).any(|w| w != r && unseen[w] >= wing)
            })
        }
        ComboKind::FourTwoSingles | ComboKind::FourTwoPairs => (3..=15).any(|r| {
            if r as u8 <= play.main_rank || unseen[r] != 4 {
                return false;
            }
            if play.kind == ComboKind::FourTwoPairs {
                (3..=15).filter(|&w| w != r && unseen[w] >= 2).count() >= 2
            } else {
                (3..=17)
                    .filter(|&w| w != r)
                    .map(|w| unseen[w] as usize)
                    .sum::<usize>()
                    >= 2
            }
        }),
        ComboKind::Straight
        | ComboKind::PairStraight
        | ComboKind::TripleStraight
        | ComboKind::AirplaneSingles
        | ComboKind::AirplanePairs => {
            let chain = play.chain;
            if chain == 0 || chain > 12 {
                return false;
            }
            let unit = match play.kind {
                ComboKind::Straight => 1,
                ComboKind::PairStraight => 2,
                _ => 3,
            };
            (3..=(15 - chain)).any(|start| {
                let end = start + chain - 1;
                if end as u8 <= play.main_rank || !(start..=end).all(|r| unseen[r] >= unit) {
                    return false;
                }
                match play.kind {
                    ComboKind::AirplaneSingles => (3..=17)
                        .filter(|&r| r < start || r > end)
                        .map(|r| unseen[r].min(3) as usize)
                        .sum::<usize>()
                        >= chain,
                    ComboKind::AirplanePairs => {
                        (3..=15)
                            .filter(|&r| (r < start || r > end) && unseen[r] >= 2)
                            .count()
                            >= chain
                    }
                    _ => true,
                }
            })
        }
        ComboKind::Bomb | ComboKind::Rocket => false,
    }
}

fn take_sequence(counts: &mut RankCounts, unit: u8, minimum: usize) -> Option<usize> {
    let mut best_start = 3;
    let mut best_length = 0;
    let mut start = 3;
    for r in 3..=15 {
        if r <= 14 && counts[r] >= unit {
            continue;
        }
        if r - start > best_length {
            best_start = start;
            best_length = r - start;
        }
        start = r + 1;
    }
    if best_length < minimum {
        return None;
    }
    for count in &mut counts[best_start..best_start + best_length] {
        *count -= unit;
    }
    Some(best_length)
}

fn greedy_groups(original: &RankCounts, order: [u8; 3]) -> i32 {
    let mut counts = *original;
    let mut groups = 0;
    for r in 3..=15 {
        if counts[r] == 4 {
            counts[r] = 0;
            groups += 1;
        }
    }
    if counts[16] == 1 && counts[17] == 1 {
        counts[16] = 0;
        counts[17] = 0;
        groups += 1;
    }
    let mut bodies = [0usize; 7];
    let mut body_count = 0;
    for unit in order {
        let minimum = match unit {
            1 => 5,
            2 => 3,
            _ => 2,
        };
        while let Some(length) = take_sequence(&mut counts, unit, minimum) {
            if unit == 3 {
                bodies[body_count] = length;
                body_count += 1;
            } else {
                groups += 1;
            }
        }
    }
    for r in 3..=15 {
        if counts[r] >= 3 {
            counts[r] -= 3;
            bodies[body_count] = 1;
            body_count += 1;
        }
    }
    bodies[..body_count].sort_unstable_by(|a, b| b.cmp(a));
    for &length in &bodies[..body_count] {
        groups += 1;
        let pairs = (3..=15).filter(|&r| counts[r] >= 2).count();
        if pairs >= length {
            let mut needed = length;
            for count in &mut counts[3..=15] {
                if *count >= 2 && needed > 0 {
                    *count -= 2;
                    needed -= 1;
                }
            }
        } else if counts.iter().map(|&count| count as usize).sum::<usize>() >= length {
            let mut needed = length;
            for count in &mut counts[3..=17] {
                let take = needed.min(*count as usize);
                *count -= take as u8;
                needed -= take;
            }
        }
    }
    for &count in &counts[3..=17] {
        groups += (count as i32 + 1) / 2;
    }
    groups
}

// Three fixed greedy decompositions estimate hand length in linear work. This
// scores grouping without searching a hidden-state game tree or card subsets.
fn group_estimate(counts: &RankCounts) -> i32 {
    [[3, 2, 1], [1, 3, 2], [2, 1, 3]]
        .into_iter()
        .map(|order| greedy_groups(counts, order))
        .min()
        .unwrap_or(0)
}

fn remaining_counts(hand: &RankCounts, play: &Combo) -> RankCounts {
    let mut remaining = *hand;
    for &id in &play.cards {
        let r = rank(id) as usize;
        remaining[r] = remaining[r].saturating_sub(1);
    }
    remaining
}

fn spend_cost(hand: &RankCounts, remaining: &RankCounts, urgent: bool) -> i32 {
    let mut cost = 0;
    for r in 3..=15 {
        let spent = hand[r] - remaining[r];
        if spent == 0 {
            continue;
        }
        if hand[r] == 4 {
            cost += if spent == 4 { 170 } else { 240 };
        } else if hand[r] >= 2 && remaining[r] > 0 {
            cost += if hand[r] == 3 { 24 } else { 14 };
        }
    }
    cost += (hand[15] - remaining[15]) as i32 * 18;
    cost += (hand[16] - remaining[16]) as i32 * 48;
    cost += (hand[17] - remaining[17]) as i32 * 65;
    if hand[16] == 1 && hand[17] == 1 && (remaining[16] == 0 || remaining[17] == 0) {
        cost += if remaining[16] != remaining[17] { 180 } else { 90 };
    }
    if urgent { cost / 4 } else { cost }
}

pub fn suggest(knowledge: &Knowledge) -> Option<Combo> {
    if knowledge.hand.is_empty() {
        return None;
    }
    let previous = knowledge
        .previous
        .as_ref()
        .filter(|_| knowledge.previous_seat != Some(knowledge.seat));
    let options = candidates(&knowledge.hand, previous);
    if options.is_empty() {
        return None;
    }
    if let Some(finisher) = options.iter().find(|play| play.length == knowledge.hand.len()) {
        return resolve(&finisher.cards, previous).ok();
    }

    let opponent_one = (0..3)
        .any(|seat| is_opponent(knowledge, seat) && knowledge.counts[seat] == 1);
    let imminent = (0..3).any(|seat| {
        is_opponent(knowledge, seat) && (1..=2).contains(&knowledge.counts[seat])
    });
    let unseen = unseen_cards(knowledge);
    let teammate = previous.is_some() && teammate_played(knowledge);
    if teammate {
        if !imminent {
            return None;
        }
        if let (Some(last), Some(landlord)) = (previous, knowledge.landlord.filter(|&seat| seat < 3)) {
            if !could_cover(last, &unseen, knowledge.counts[landlord]) {
                return None;
            }
        }
    }

    let hand = rank_counts(&knowledge.hand);
    let can_avoid_single = options.iter().any(|play| play.kind != ComboKind::Single);
    let mut best: Option<(i32, usize, Combo)> = None;
    for play in options {
        let remaining = remaining_counts(&hand, &play);
        let groups = group_estimate(&remaining);
        let left = knowledge.hand.len() - play.length;
        let mut score = groups * 110 + left as i32 * 3 - play.length as i32 * 2;
        score += spend_cost(&hand, &remaining, opponent_one);
        score += play.main_rank as i32;
        score += (3..=10).filter(|&r| remaining[r] == 1).count() as i32 * 5;

        if opponent_one {
            if previous.is_none() && play.kind == ComboKind::Single && can_avoid_single {
                score += 500;
            }
            if play.kind == ComboKind::Single {
                score -= play.main_rank as i32 * 24;
            }
        }
        if imminent {
            let exposed = (0..3).any(|seat| {
                is_opponent(knowledge, seat)
                    && (1..=2).contains(&knowledge.counts[seat])
                    && could_cover(&play, &unseen, knowledge.counts[seat])
            });
            score += if exposed { 260 } else { -70 };
        }

        // Farmers can hand the lead to a nearly finished teammate by leading a
        // low single/pair matching that teammate's publicly visible hand count.
        if previous.is_none() && knowledge.seat != knowledge.landlord.unwrap_or(knowledge.seat) {
            if let Some(friend) = (0..3).find(|&seat| {
                seat != knowledge.seat
                    && Some(seat) != knowledge.landlord
                    && (1..=2).contains(&knowledge.counts[seat])
            }) {
                let matching = (knowledge.counts[friend] == 1 && play.kind == ComboKind::Single)
                    || (knowledge.counts[friend] == 2 && play.kind == ComboKind::Pair);
                if matching && !imminent {
                    score -= 55;
                    score += play.main_rank as i32 * 3;
                }
            }
        }

        let replace = best.as_ref().map_or(true, |(best_score, best_left, _)| {
            score < *best_score || (score == *best_score && left < *best_left)
        });
        if replace {
            best = Some((score, left, play));
        }
    }
    let (_, _, choice) = best?;
    if previous.is_some() && !imminent {
        let remaining = remaining_counts(&hand, &choice);
        let expensive = matches!(choice.kind, ComboKind::Bomb | ComboKind::Rocket)
            || (hand[16] == 1 && hand[17] == 1 && remaining[16] != remaining[17]);
        if expensive && group_estimate(&remaining) > 1 {
            return None;
        }
    }
    resolve(&choice.cards, previous).ok()
}

fn bidding_strength(hand: &[u8]) -> i32 {
    let counts = rank_counts(hand);
    let mut strength = 0;
    strength += counts[17] as i32 * 5;
    strength += counts[16] as i32 * 3;
    strength += counts[15] as i32 * 2;
    strength += counts[14] as i32;
    if counts[16] == 1 && counts[17] == 1 {
        strength += 5;
    }
    strength += (3..=15).filter(|&r| counts[r] == 4).count() as i32 * 5;
    strength += (3..=14).filter(|&r| counts[r] == 3).count() as i32;
    strength - (group_estimate(&counts) - 7).max(0)
}

pub fn bid_score(hand: &[u8], minimum: u8) -> u8 {
    if !valid_hand(hand) || minimum >= 3 {
        return 0;
    }
    let strength = bidding_strength(hand);
    let bid = if strength >= 15 {
        3
    } else if strength >= 10 {
        2
    } else if strength >= 6 {
        1
    } else {
        0
    };
    if bid > minimum { bid } else { 0 }
}

pub fn want_landlord(hand: &[u8], robbing: bool) -> bool {
    if !valid_hand(hand) {
        return false;
    }
    bidding_strength(hand) >= if robbing { 10 } else { 6 }
}
