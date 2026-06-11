//! Small FAWE-style block pattern and mask subset used by commands.
//!
//! FAWE's real parser accepts many rich expressions. This module intentionally
//! implements the pieces this plugin can apply with only block-state ids:
//! literal blocks, `#existing`, simple weighted mixes, and comma-separated
//! literal masks.

use pumpkin_plugin_api::common::BlockPos;

use crate::mapping;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockPattern {
    Literal {
        input: String,
        state_id: u16,
    },
    Existing,
    Weighted {
        input: String,
        entries: Vec<WeightedBlock>,
        total: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeightedBlock {
    pub weight: u32,
    pub input: String,
    pub state_id: u16,
}

impl BlockPattern {
    pub fn parse(input: &str) -> Result<Self, String> {
        let input = input.trim();
        if input.is_empty() {
            return Err("Expected a block pattern.".to_string());
        }

        if input.eq_ignore_ascii_case("#existing") {
            return Ok(Self::Existing);
        }
        if input.starts_with('#') {
            return Err(format!(
                "Pattern '{input}' needs FAWE's full pattern engine, which is not implemented yet."
            ));
        }

        if input.contains(',') || input.contains('%') {
            return parse_weighted(input);
        }

        let Some(state_id) = mapping::resolve_block(input) else {
            return Err(format!("Unknown block '{input}'."));
        };
        Ok(Self::Literal {
            input: input.to_string(),
            state_id,
        })
    }

    pub fn state_at(&self, pos: BlockPos, before: u16) -> u16 {
        match self {
            Self::Literal { state_id, .. } => *state_id,
            Self::Existing => before,
            Self::Weighted { entries, total, .. } => {
                let mut pick = position_hash(pos) % *total;
                for entry in entries {
                    if pick < entry.weight {
                        return entry.state_id;
                    }
                    pick -= entry.weight;
                }
                entries.last().map_or(before, |entry| entry.state_id)
            }
        }
    }

    pub fn literal_display(&self) -> Option<(&str, u16)> {
        match self {
            Self::Literal { input, state_id } => Some((input, *state_id)),
            _ => None,
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Self::Literal { input, .. } => input,
            Self::Existing => "#existing",
            Self::Weighted { input, .. } => input,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockMask {
    states: Vec<u16>,
}

impl BlockMask {
    pub fn parse(input: &str) -> Result<Self, String> {
        let input = input.trim();
        if input.is_empty() {
            return Err("Expected a mask after -m.".to_string());
        }
        if input.starts_with('#') || input.contains('%') {
            return Err(format!(
                "Mask '{input}' needs FAWE's full mask parser, which is not implemented yet."
            ));
        }

        let mut states = Vec::new();
        for raw in input.split(',') {
            let block = raw.trim();
            if block.is_empty() {
                return Err(format!("Invalid empty block in mask '{input}'."));
            }
            let Some(state_id) = mapping::resolve_block(block) else {
                return Err(format!("Unknown block '{block}'."));
            };
            if !states.contains(&state_id) {
                states.push(state_id);
            }
        }

        Ok(Self { states })
    }

    pub fn matches(&self, state_id: u16) -> bool {
        self.states.contains(&state_id)
    }
}

fn parse_weighted(input: &str) -> Result<BlockPattern, String> {
    let mut entries = Vec::new();
    let mut total = 0u32;

    for raw in input.split(',') {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(format!("Invalid empty entry in pattern '{input}'."));
        }

        let (weight, block) = match raw.split_once('%') {
            Some((weight, block)) => {
                let weight = parse_weight(weight.trim(), input)?;
                (weight, block.trim())
            }
            None => (1000, raw),
        };

        if block.is_empty() {
            return Err(format!("Missing block after weight in pattern '{input}'."));
        }
        let Some(state_id) = mapping::resolve_block(block) else {
            return Err(format!("Unknown block '{block}'."));
        };

        total = total
            .checked_add(weight)
            .ok_or_else(|| format!("Pattern '{input}' has too much total weight."))?;
        entries.push(WeightedBlock {
            weight,
            input: block.to_string(),
            state_id,
        });
    }

    if entries.len() == 1 {
        let entry = entries.remove(0);
        return Ok(BlockPattern::Literal {
            input: entry.input,
            state_id: entry.state_id,
        });
    }

    Ok(BlockPattern::Weighted {
        input: input.to_string(),
        entries,
        total,
    })
}

fn parse_weight(raw: &str, whole: &str) -> Result<u32, String> {
    let value = raw
        .parse::<f64>()
        .map_err(|_| format!("Invalid weight '{raw}' in pattern '{whole}'."))?;
    if !value.is_finite() || value <= 0.0 {
        return Err(format!("Invalid weight '{raw}' in pattern '{whole}'."));
    }
    Ok((value * 1000.0).round().max(1.0) as u32)
}

fn position_hash(pos: BlockPos) -> u32 {
    let mut x = pos.x as u32;
    x ^= (pos.y as u32).wrapping_mul(0x9e37_79b9);
    x = x.rotate_left(13);
    x ^= (pos.z as u32).wrapping_mul(0x85eb_ca6b);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos { x, y, z }
    }

    #[test]
    fn parses_literal_pattern() {
        let pattern = BlockPattern::parse("stone").unwrap();
        assert_eq!(pattern.state_at(at(0, 0, 0), 10), 1);
        assert_eq!(pattern.literal_display(), Some(("stone", 1)));
    }

    #[test]
    fn existing_pattern_keeps_before_state() {
        let pattern = BlockPattern::parse("#existing").unwrap();
        assert_eq!(pattern.state_at(at(0, 0, 0), 10), 10);
    }

    #[test]
    fn parses_weighted_pattern() {
        let pattern = BlockPattern::parse("50%stone,50%dirt").unwrap();
        match pattern {
            BlockPattern::Weighted { total, entries, .. } => {
                assert_eq!(total, 100_000);
                assert_eq!(entries.len(), 2);
            }
            _ => panic!("expected weighted pattern"),
        }
    }

    #[test]
    fn parses_literal_mask_list() {
        let mask = BlockMask::parse("stone,dirt").unwrap();
        assert!(mask.matches(1));
        assert!(mask.matches(10));
        assert!(!mask.matches(0));
    }
}
