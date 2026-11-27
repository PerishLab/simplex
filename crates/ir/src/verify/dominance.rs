use crate::{Block, Terminator};
use std::collections::HashMap;

pub struct Tree {
    pub map: HashMap<u32, usize>,
    pub reachable: Vec<bool>,
    pub enter: Vec<u32>,
    pub leave: Vec<u32>,
}

impl Tree {
    pub fn build(blocks: &[Block], map: HashMap<u32, usize>) -> Self {
        let count = blocks.len();
        let (order, rank, reachable) = walk(blocks, &map);
        let preds = predecessors(blocks, &map, &reachable);
        let parent = dominators(&order, &rank, &preds, count);
        let (enter, leave) = stamp(&order, &parent, count);
        Self {
            map,
            reachable,
            enter,
            leave,
        }
    }

    pub fn dominates(&self, source: Option<usize>, target: usize) -> bool {
        let Some(origin) = source else {
            return true;
        };
        if !self.reachable[target] {
            return true;
        }
        if !self.reachable[origin] {
            return false;
        }
        self.enter[origin] <= self.enter[target] && self.leave[target] <= self.leave[origin]
    }
}

fn successors(exit: &Terminator, out: &mut Vec<u32>) {
    out.clear();
    match exit {
        Terminator::Jump(target) => out.push(target.block),
        Terminator::Branch { truthy, falsy, .. } => {
            out.push(truthy.block);
            out.push(falsy.block);
        }
        Terminator::Switch { cases, default, .. } => {
            for (_, item) in cases {
                out.push(item.block);
            }
            out.push(default.block);
        }
        Terminator::Return(_) | Terminator::Trap { .. } => {}
    }
}

fn walk(blocks: &[Block], map: &HashMap<u32, usize>) -> (Vec<usize>, Vec<usize>, Vec<bool>) {
    let count = blocks.len();
    let mut visited = vec![false; count];
    let mut post = Vec::with_capacity(count);
    let mut stack: Vec<(usize, bool)> = vec![(0, false)];
    let mut edges = Vec::new();
    visited[0] = true;
    while let Some((node, expanded)) = stack.pop() {
        if expanded {
            post.push(node);
            continue;
        }
        stack.push((node, true));
        successors(&blocks[node].exit, &mut edges);
        for &id in edges.iter().rev() {
            visit(map.get(&id).copied(), &mut visited, &mut stack);
        }
    }
    post.reverse();
    let mut rank = vec![usize::MAX; count];
    for (pos, &node) in post.iter().enumerate() {
        rank[node] = pos;
    }
    (post, rank, visited)
}

fn visit(target: Option<usize>, visited: &mut [bool], stack: &mut Vec<(usize, bool)>) {
    if let Some(next) = target
        && !visited[next]
    {
        visited[next] = true;
        stack.push((next, false));
    }
}

fn predecessors(
    blocks: &[Block],
    map: &HashMap<u32, usize>,
    reachable: &[bool],
) -> Vec<Vec<usize>> {
    let mut preds = vec![Vec::new(); blocks.len()];
    let mut edges = Vec::new();
    for (idx, block) in blocks.iter().enumerate() {
        if !reachable[idx] {
            continue;
        }
        successors(&block.exit, &mut edges);
        for &id in &edges {
            if let Some(&dst) = map.get(&id) {
                preds[dst].push(idx);
            }
        }
    }
    preds
}

fn dominators(order: &[usize], rank: &[usize], preds: &[Vec<usize>], count: usize) -> Vec<usize> {
    let mut parent = vec![usize::MAX; count];
    parent[0] = 0;
    for _ in 0..8 {
        if !step(order, rank, preds, &mut parent) {
            break;
        }
    }
    for &node in order.iter().skip(1) {
        if parent[node] == usize::MAX {
            parent[node] = 0;
        }
    }
    parent
}

fn step(order: &[usize], rank: &[usize], preds: &[Vec<usize>], parent: &mut [usize]) -> bool {
    let mut changed = false;
    for &node in order.iter().skip(1) {
        let candidate = fold(&preds[node], rank, parent);
        if candidate != usize::MAX && parent[node] != candidate {
            parent[node] = candidate;
            changed = true;
        }
    }
    changed
}

fn fold(incoming: &[usize], rank: &[usize], parent: &[usize]) -> usize {
    let mut acc = usize::MAX;
    for &pred in incoming {
        if parent[pred] == usize::MAX {
            continue;
        }
        acc = match acc == usize::MAX {
            true => pred,
            false => intersect((acc, pred), rank, parent),
        };
    }
    acc
}

fn intersect(pair: (usize, usize), rank: &[usize], parent: &[usize]) -> usize {
    let (mut left, mut right) = pair;
    while left != right {
        while rank[left] > rank[right] {
            left = parent[left];
        }
        while rank[right] > rank[left] {
            right = parent[right];
        }
    }
    left
}

fn stamp(order: &[usize], parent: &[usize], count: usize) -> (Vec<u32>, Vec<u32>) {
    let mut children = vec![Vec::new(); count];
    for &node in order.iter().skip(1) {
        let dom = parent[node];
        children[dom].push(node);
    }
    let mut enter = vec![0u32; count];
    let mut leave = vec![0u32; count];
    let mut clock = 0u32;
    let mut stack = vec![(0usize, false)];
    while let Some((node, done)) = stack.pop() {
        clock += 1;
        if done {
            leave[node] = clock;
            continue;
        }
        enter[node] = clock;
        stack.push((node, true));
        enqueue(&children[node], &mut stack);
    }
    (enter, leave)
}

fn enqueue(items: &[usize], stack: &mut Vec<(usize, bool)>) {
    for &child in items.iter().rev() {
        stack.push((child, false));
    }
}
