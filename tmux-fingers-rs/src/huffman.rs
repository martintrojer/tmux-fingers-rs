use crate::priority_queue::PriorityQueue;

#[derive(Debug, Clone)]
struct HuffmanNode {
    weight: i32,
    children: Vec<HuffmanNode>,
}

impl HuffmanNode {
    fn new(weight: i32, children: Vec<HuffmanNode>) -> Self {
        Self { weight, children }
    }
}

#[derive(Debug, Default)]
pub struct Huffman;

impl Huffman {
    pub fn generate_hints(&self, alphabet: &[String], n: usize) -> Vec<String> {
        if n <= alphabet.len() {
            return alphabet.to_vec();
        }

        let arity = alphabet.len();
        let mut queue = PriorityQueue::new();
        for i in 0..n {
            queue.push(-(i as i32), HuffmanNode::new(-(i as i32), Vec::new()));
        }

        let mut first_node = true;
        while queue.len() > 1 {
            let n_branches = if first_node {
                first_node = false;
                initial_number_of_branches(n, arity)
            } else {
                arity
            };

            let mut smallest = Vec::new();
            for _ in 0..n_branches.min(queue.len()) {
                smallest.push(queue.pop().expect("queue underflow"));
            }

            let weight = smallest.iter().map(|node| node.weight).sum();
            queue.push(weight, HuffmanNode::new(weight, smallest));
        }

        let root = queue.pop().expect("missing root");
        let mut result = Vec::new();
        traverse_tree(&root, &mut Vec::new(), &mut |node, path| {
            if node.children.is_empty() {
                result.push(translate_path(path, alphabet));
            }
        });
        result.sort_by_key(|hint| hint.len());
        result
    }
}

fn initial_number_of_branches(n: usize, arity: usize) -> usize {
    let mut result = 1usize;

    for t in 1..=(n / arity + 1) {
        result = n.saturating_sub(t * (arity - 1));
        if (2..=arity).contains(&result) {
            break;
        }
        result = arity;
    }

    result
}

fn traverse_tree(
    node: &HuffmanNode,
    path: &mut Vec<usize>,
    visit: &mut impl FnMut(&HuffmanNode, &[usize]),
) {
    visit(node, path);
    for (index, child) in node.children.iter().enumerate() {
        path.push(index);
        traverse_tree(child, path, visit);
        path.pop();
    }
}

fn translate_path(path: &[usize], alphabet: &[String]) -> String {
    path.iter().map(|index| alphabet[*index].as_str()).collect()
}

#[cfg(test)]
mod tests {
    use super::Huffman;

    #[test]
    fn generates_hints_for_5() {
        let expected = vec!["s", "d", "f", "aa", "as"];
        let alphabet = vec!["a", "s", "d", "f"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();

        let result = Huffman.generate_hints(&alphabet, 5);
        assert_eq!(result, expected);
    }

    #[test]
    fn generates_hints_for_50() {
        let expected = vec![
            "aaa", "aas", "aad", "aaf", "asa", "ass", "asd", "asf", "ada", "ads", "add", "adf",
            "afa", "afd", "aff", "saa", "sas", "sad", "saf", "ssa", "sss", "ssd", "ssf", "sda",
            "sds", "sdd", "sdf", "sfa", "afsa", "afss", "afsd", "afsf", "sfsa", "sfss", "sfsd",
            "sfsf", "sfda", "sfds", "sfdd", "sfdf", "sffa", "sffs", "sffd", "sfffa", "sfffs",
            "sfffd", "sffffa", "sffffs", "sffffd", "sfffff",
        ];
        let alphabet = vec!["a", "s", "d", "f"]
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();

        let result = Huffman.generate_hints(&alphabet, 50);
        assert_eq!(result, expected);
    }
}
