use crate::digest::align;

#[derive(Default)]
struct Node {
    terminal: Option<u64>,
    children: Vec<(Vec<u8>, usize)>,
}

pub fn build(exports: &[(String, u64)]) -> Vec<u8> {
    if exports.is_empty() {
        return vec![0u8; 8];
    }
    let mut nodes = vec![Node::default()];
    for (name, offset) in exports {
        if !name.is_empty() {
            insert(&mut nodes, 0, name.as_bytes(), *offset);
        }
    }
    let mut offsets = vec![0u64; nodes.len()];
    loop {
        let mut next = vec![0u64; nodes.len()];
        let mut cursor = 0u64;
        for (index, node) in nodes.iter().enumerate() {
            next[index] = cursor;
            cursor += measure(node, &offsets);
        }
        if next == offsets {
            break;
        }
        offsets = next;
    }
    let mut out = Vec::new();
    for node in &nodes {
        encode(&mut out, node, &offsets);
    }
    let padded = align(out.len() as u64, 8) as usize;
    out.resize(padded, 0);
    out
}

fn insert(nodes: &mut Vec<Node>, current: usize, key: &[u8], value: u64) {
    if key.is_empty() {
        nodes[current].terminal = Some(value);
        return;
    }
    let found = nodes[current]
        .children
        .iter()
        .position(|(edge, _)| edge[0] == key[0]);
    let Some(position) = found else {
        let leaf = nodes.len();
        nodes.push(Node {
            terminal: Some(value),
            children: Vec::new(),
        });
        nodes[current].children.push((key.to_vec(), leaf));
        nodes[current].children.sort_by_key(|(edge, _)| edge[0]);
        return;
    };
    let (edge, target) = nodes[current].children[position].clone();
    let shared = common(&edge, key);
    if shared == edge.len() {
        insert(nodes, target, &key[shared..], value);
        return;
    }
    let split = nodes.len();
    nodes.push(Node {
        terminal: None,
        children: vec![(edge[shared..].to_vec(), target)],
    });
    nodes[current].children[position] = (edge[..shared].to_vec(), split);
    insert(nodes, split, &key[shared..], value);
}

fn common(left: &[u8], right: &[u8]) -> usize {
    left.iter()
        .zip(right.iter())
        .take_while(|(first, second)| first == second)
        .count()
}

fn measure(node: &Node, offsets: &[u64]) -> u64 {
    let mut total = match node.terminal {
        None => 1,
        Some(value) => {
            let payload = 1 + width(value);
            width(payload) + payload
        }
    };
    total += 1;
    for (edge, child) in &node.children {
        total += (edge.len() as u64) + 1 + width(offsets[*child]);
    }
    total
}

fn encode(out: &mut Vec<u8>, node: &Node, offsets: &[u64]) {
    match node.terminal {
        None => out.push(0),
        Some(value) => {
            let payload = 1 + width(value);
            pack(out, payload);
            out.push(0);
            pack(out, value);
        }
    }
    out.push(node.children.len() as u8);
    for (edge, child) in &node.children {
        out.extend_from_slice(edge);
        out.push(0);
        pack(out, offsets[*child]);
    }
}

fn width(mut value: u64) -> u64 {
    let mut count = 0u64;
    loop {
        value >>= 7;
        count += 1;
        if value == 0 {
            return count;
        }
    }
}

fn pack(out: &mut Vec<u8>, mut value: u64) {
    loop {
        let mut byte = (value & 0x7F) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}
