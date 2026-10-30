use std::cmp::Ordering;
use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::cards::{rank, sort};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComboKind {
    Single,
    Pair,
    Triple,
    TripleSingle,
    TriplePair,
    Straight,
    PairStraight,
    TripleStraight,
    AirplaneSingles,
    AirplanePairs,
    FourTwoSingles,
    FourTwoPairs,
    Bomb,
    Rocket,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Combo {
    pub kind: ComboKind,
    pub main_rank: u8,
    pub length: usize,
    pub chain: usize,
    pub label: String,
    pub cards: Vec<u8>,
}

type RankCounts = [u8; 18];

fn validated_counts(ids: &[u8]) -> Result<RankCounts, String> {
    if ids.is_empty() || ids.len() > 20 {
        return Err("请选择 1 至 20 张牌".into());
    }
    let mut seen = 0u64;
    let mut counts = [0; 18];
    for &id in ids {
        if id >= 54 {
            return Err("包含无效的牌".into());
        }
        let bit = 1u64 << id;
        if seen & bit != 0 {
            return Err("同一张牌不能重复使用".into());
        }
        seen |= bit;
        counts[rank(id) as usize] += 1;
    }
    Ok(counts)
}

fn label(kind: ComboKind) -> &'static str {
    match kind {
        ComboKind::Single => "单张",
        ComboKind::Pair => "对子",
        ComboKind::Triple => "三张",
        ComboKind::TripleSingle => "三带一",
        ComboKind::TriplePair => "三带二",
        ComboKind::Straight => "顺子",
        ComboKind::PairStraight => "连对",
        ComboKind::TripleStraight => "三顺",
        ComboKind::AirplaneSingles => "飞机带单",
        ComboKind::AirplanePairs => "飞机带对",
        ComboKind::FourTwoSingles => "四带二",
        ComboKind::FourTwoPairs => "四带两对",
        ComboKind::Bomb => "炸弹",
        ComboKind::Rocket => "王炸",
    }
}

fn make_combo(kind: ComboKind, main_rank: u8, chain: usize, cards: &[u8]) -> Combo {
    Combo {
        kind,
        main_rank,
        length: cards.len(),
        chain,
        label: label(kind).into(),
        cards: cards.to_vec(),
    }
}

fn kind_order(kind: ComboKind) -> u8 {
    match kind {
        ComboKind::Single => 0,
        ComboKind::Pair => 1,
        ComboKind::Triple => 2,
        ComboKind::TripleSingle => 3,
        ComboKind::TriplePair => 4,
        ComboKind::Straight => 5,
        ComboKind::PairStraight => 6,
        ComboKind::TripleStraight => 7,
        ComboKind::AirplaneSingles => 8,
        ComboKind::AirplanePairs => 9,
        ComboKind::FourTwoSingles => 10,
        ComboKind::FourTwoPairs => 11,
        ComboKind::Bomb => 12,
        ComboKind::Rocket => 13,
    }
}

fn triple_body(combo: &Combo) -> usize {
    match combo.kind {
        ComboKind::TripleStraight | ComboKind::AirplaneSingles | ComboKind::AirplanePairs => {
            combo.chain
        }
        _ => 0,
    }
}

fn canonical_order(a: &Combo, b: &Combo) -> Ordering {
    triple_body(b)
        .cmp(&triple_body(a))
        .then_with(|| kind_order(a.kind).cmp(&kind_order(b.kind)))
        .then_with(|| a.main_rank.cmp(&b.main_rank))
        .then_with(|| a.cards.cmp(&b.cards))
}

fn sequence(counts: &RankCounts, unit: u8, minimum: usize) -> Option<(u8, usize)> {
    if counts[15..].iter().any(|&count| count != 0) {
        return None;
    }
    let occupied: Vec<usize> = (3..=14).filter(|&r| counts[r] != 0).collect();
    if occupied.len() < minimum {
        return None;
    }
    let first = occupied[0];
    let last = occupied[occupied.len() - 1];
    if last - first + 1 != occupied.len()
        || occupied.iter().any(|&r| counts[r] != unit)
    {
        return None;
    }
    Some((last as u8, occupied.len()))
}

pub fn classify(ids: &[u8]) -> Result<Vec<Combo>, String> {
    let counts = validated_counts(ids)?;
    let mut cards = ids.to_vec();
    sort(&mut cards);
    let n = cards.len();
    let mut result = Vec::new();

    if n == 2 && counts[16] == 1 && counts[17] == 1 {
        result.push(make_combo(ComboKind::Rocket, 17, 1, &cards));
    }
    for r in 3..=17 {
        let main = r as u8;
        match (n, counts[r]) {
            (1, 1) => result.push(make_combo(ComboKind::Single, main, 1, &cards)),
            (2, 2) => result.push(make_combo(ComboKind::Pair, main, 1, &cards)),
            (3, 3) => result.push(make_combo(ComboKind::Triple, main, 1, &cards)),
            (4, 4) => result.push(make_combo(ComboKind::Bomb, main, 1, &cards)),
            (4, 3) => result.push(make_combo(ComboKind::TripleSingle, main, 1, &cards)),
            (5, 3) if counts.iter().enumerate().any(|(w, &c)| w != r && c == 2) => {
                result.push(make_combo(ComboKind::TriplePair, main, 1, &cards));
            }
            (6, 4) => result.push(make_combo(ComboKind::FourTwoSingles, main, 1, &cards)),
            (8, 4)
                if counts
                    .iter()
                    .enumerate()
                    .filter(|&(w, &c)| w != r && c == 2)
                    .count()
                    == 2 =>
            {
                result.push(make_combo(ComboKind::FourTwoPairs, main, 1, &cards));
            }
            _ => {}
        }
    }

    for (unit, minimum, kind) in [
        (1, 5, ComboKind::Straight),
        (2, 3, ComboKind::PairStraight),
        (3, 2, ComboKind::TripleStraight),
    ] {
        if let Some((main, chain)) = sequence(&counts, unit, minimum) {
            result.push(make_combo(kind, main, chain, &cards));
        }
    }

    // The selected cards may admit multiple consecutive triple bodies. Keep every
    // interpretation so following a play can choose the body that actually beats it.
    for (width, kind) in [
        (4, ComboKind::AirplaneSingles),
        (5, ComboKind::AirplanePairs),
    ] {
        if n % width != 0 {
            continue;
        }
        let chain = n / width;
        if chain < 2 || chain > 12 {
            continue;
        }
        for start in 3..=(15 - chain) {
            let end = start + chain - 1;
            if !(start..=end).all(|r| counts[r] == 3) {
                continue;
            }
            let mut wings = 0usize;
            let valid_wings = (3..=17).filter(|r| *r < start || *r > end).all(|r| {
                let count = counts[r];
                wings += count as usize;
                match kind {
                    ComboKind::AirplaneSingles => count <= 3,
                    ComboKind::AirplanePairs => count == 0 || count == 2,
                    _ => false,
                }
            });
            let required = if kind == ComboKind::AirplanePairs {
                2 * chain
            } else {
                chain
            };
            if valid_wings && wings == required {
                result.push(make_combo(kind, end as u8, chain, &cards));
            }
        }
    }

    if result.is_empty() {
        return Err("这些牌不能组成合法牌型".into());
    }
    result.sort_by(canonical_order);
    result.dedup_by(|a, b| a.kind == b.kind && a.main_rank == b.main_rank && a.chain == b.chain);
    Ok(result)
}

pub fn beats(candidate: &Combo, previous: &Combo) -> bool {
    shape_beats(
        candidate.kind,
        candidate.main_rank,
        candidate.length,
        candidate.chain,
        previous,
    )
}

fn shape_beats(
    kind: ComboKind,
    main_rank: u8,
    length: usize,
    chain: usize,
    previous: &Combo,
) -> bool {
    if previous.kind == ComboKind::Rocket {
        return false;
    }
    if kind == ComboKind::Rocket {
        return true;
    }
    if kind == ComboKind::Bomb {
        return previous.kind != ComboKind::Bomb || main_rank > previous.main_rank;
    }
    previous.kind != ComboKind::Bomb
        && kind == previous.kind
        && length == previous.length
        && chain == previous.chain
        && main_rank > previous.main_rank
}

pub fn resolve(ids: &[u8], previous: Option<&Combo>) -> Result<Combo, String> {
    let interpretations = classify(ids)?;
    interpretations
        .into_iter()
        .find(|combo| previous.map_or(true, |last| beats(combo, last)))
        .ok_or_else(|| "所选牌型无法压过上一手牌".into())
}

struct Generator<'a> {
    buckets: [Vec<u8>; 18],
    previous: Option<&'a Combo>,
    seen: HashSet<RankCounts>,
    output: Vec<Combo>,
}

impl<'a> Generator<'a> {
    fn allows(&self, kind: ComboKind, main: usize, length: usize, chain: usize) -> bool {
        self.previous
            .map_or(true, |last| shape_beats(kind, main as u8, length, chain, last))
    }

    fn add(&mut self, template: &RankCounts) {
        if !self.seen.insert(*template) {
            return;
        }
        let mut ids = Vec::new();
        for (r, &count) in template.iter().enumerate().skip(3) {
            let count = count as usize;
            if count > self.buckets[r].len() {
                return;
            }
            ids.extend_from_slice(&self.buckets[r][..count]);
        }
        // Templates choose ranks; classification is the sole source of legal
        // interpretations, including ambiguous airplane bodies.
        if let Ok(combos) = classify(&ids) {
            let previous = self.previous;
            self.output.extend(
                combos
                    .into_iter()
                    .filter(|combo| previous.map_or(true, |last| beats(combo, last))),
            );
        }
    }
}

fn single_wings(
    counts: &RankCounts,
    template: &mut RankCounts,
    needed: usize,
    generator: &mut Generator<'_>,
) {
    let options: Vec<(usize, u8)> = (3..=17)
        .filter(|&r| template[r] == 0 && counts[r] != 0)
        .map(|r| (r, counts[r].min(3)))
        .collect();
    let mut suffix = vec![0; options.len() + 1];
    for index in (0..options.len()).rev() {
        suffix[index] = suffix[index + 1] + options[index].1 as usize;
    }
    allocate_singles(&options, &suffix, 0, needed, template, generator);
}

fn allocate_singles(
    options: &[(usize, u8)],
    suffix: &[usize],
    index: usize,
    needed: usize,
    template: &mut RankCounts,
    generator: &mut Generator<'_>,
) {
    if needed == 0 {
        generator.add(template);
        return;
    }
    if index == options.len() || suffix[index] < needed {
        return;
    }
    let (r, available) = options[index];
    let maximum = needed.min(available as usize);
    let minimum = needed.saturating_sub(suffix[index + 1]);
    for take in minimum..=maximum {
        template[r] = take as u8;
        allocate_singles(options, suffix, index + 1, needed - take, template, generator);
    }
    template[r] = 0;
}

fn pair_wings(
    counts: &RankCounts,
    template: &mut RankCounts,
    needed: usize,
    generator: &mut Generator<'_>,
) {
    let options: Vec<usize> = (3..=15)
        .filter(|&r| template[r] == 0 && counts[r] >= 2)
        .collect();
    allocate_pairs(&options, 0, needed, template, generator);
}

fn allocate_pairs(
    options: &[usize],
    index: usize,
    needed: usize,
    template: &mut RankCounts,
    generator: &mut Generator<'_>,
) {
    if needed == 0 {
        generator.add(template);
        return;
    }
    if options.len() - index < needed {
        return;
    }
    for position in index..=(options.len() - needed) {
        let r = options[position];
        template[r] = 2;
        allocate_pairs(options, position + 1, needed - 1, template, generator);
        template[r] = 0;
    }
}

pub fn candidates(hand: &[u8], previous: Option<&Combo>) -> Vec<Combo> {
    let counts = match validated_counts(hand) {
        Ok(counts) => counts,
        Err(_) => return Vec::new(),
    };
    let mut buckets: [Vec<u8>; 18] = std::array::from_fn(|_| Vec::new());
    for &id in hand {
        buckets[rank(id) as usize].push(id);
    }
    for bucket in &mut buckets {
        bucket.sort_unstable_by(|a, b| b.cmp(a));
    }
    let mut generator = Generator {
        buckets,
        previous,
        seen: HashSet::new(),
        output: Vec::new(),
    };

    for r in 3..=17 {
        for (unit, kind) in [
            (1, ComboKind::Single),
            (2, ComboKind::Pair),
            (3, ComboKind::Triple),
            (4, ComboKind::Bomb),
        ] {
            if counts[r] >= unit && generator.allows(kind, r, unit as usize, 1) {
                let mut template = [0; 18];
                template[r] = unit;
                generator.add(&template);
            }
        }
        if counts[r] >= 3 {
            for (wing_size, kind) in [(1, ComboKind::TripleSingle), (2, ComboKind::TriplePair)] {
                if !generator.allows(kind, r, 3 + wing_size as usize, 1) {
                    continue;
                }
                for wing in 3..=17 {
                    if wing != r && counts[wing] >= wing_size {
                        let mut template = [0; 18];
                        template[r] = 3;
                        template[wing] = wing_size;
                        generator.add(&template);
                    }
                }
            }
        }
        if counts[r] == 4 {
            let mut template = [0; 18];
            template[r] = 4;
            if generator.allows(ComboKind::FourTwoSingles, r, 6, 1) {
                single_wings(&counts, &mut template, 2, &mut generator);
            }
            if generator.allows(ComboKind::FourTwoPairs, r, 8, 1) {
                pair_wings(&counts, &mut template, 2, &mut generator);
            }
        }
    }

    if counts[16] == 1 && counts[17] == 1 && generator.allows(ComboKind::Rocket, 17, 2, 1) {
        let mut template = [0; 18];
        template[16] = 1;
        template[17] = 1;
        generator.add(&template);
    }

    for (unit, minimum, kind) in [
        (1, 5, ComboKind::Straight),
        (2, 3, ComboKind::PairStraight),
        (3, 2, ComboKind::TripleStraight),
    ] {
        for start in 3..=14 {
            let mut template = [0; 18];
            for end in start..=14 {
                if counts[end] < unit {
                    break;
                }
                template[end] = unit;
                let chain = end - start + 1;
                let length = chain * unit as usize;
                if chain < minimum {
                    continue;
                }
                if generator.allows(kind, end, length, chain) {
                    generator.add(&template);
                }
                if unit == 3 {
                    if 4 * chain <= hand.len()
                        && generator.allows(ComboKind::AirplaneSingles, end, 4 * chain, chain)
                    {
                        single_wings(&counts, &mut template, chain, &mut generator);
                    }
                    if 5 * chain <= hand.len()
                        && generator.allows(ComboKind::AirplanePairs, end, 5 * chain, chain)
                    {
                        pair_wings(&counts, &mut template, chain, &mut generator);
                    }
                }
            }
        }
    }

    generator.output.sort_by(|a, b| {
        b.length
            .cmp(&a.length)
            .then_with(|| canonical_order(a, b))
    });
    generator.output
}
