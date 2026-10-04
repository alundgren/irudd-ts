use crate::dryer::{CloneGroup, ClonePair};
use crate::quality::FunctionLocation;
use std::collections::BTreeMap;

fn identity(function: &FunctionLocation) -> (&str, usize, usize) {
    let location = &function.location;
    (&location.file, location.start, location.end)
}

fn root(parents: &mut [usize], mut node: usize) -> usize {
    while parents[node] != node {
        parents[node] = parents[parents[node]];
        node = parents[node];
    }
    node
}

pub(super) fn group_pairs(pairs: &[ClonePair]) -> Vec<CloneGroup> {
    let mut members = BTreeMap::new();
    for pair in pairs {
        for function in [&pair.left, &pair.right] {
            members.entry(identity(function)).or_insert(function);
        }
    }
    let indices: BTreeMap<_, _> = members
        .keys()
        .enumerate()
        .map(|(index, key)| (*key, index))
        .collect();
    let mut parents: Vec<_> = (0..members.len()).collect();
    for pair in pairs {
        let left = root(&mut parents, indices[&identity(&pair.left)]);
        let right = root(&mut parents, indices[&identity(&pair.right)]);
        parents[left.max(right)] = left.min(right);
    }
    let mut groups = BTreeMap::<usize, CloneGroup>::new();
    for (index, function) in members.into_values().enumerate() {
        groups
            .entry(root(&mut parents, index))
            .or_insert_with(|| CloneGroup {
                members: vec![],
                pair_indices: vec![],
                all_members_match: false,
            })
            .members
            .push(function.clone());
    }
    for (index, pair) in pairs.iter().enumerate() {
        groups
            .get_mut(&root(&mut parents, indices[&identity(&pair.left)]))
            .unwrap()
            .pair_indices
            .push(index);
    }
    groups
        .into_values()
        .map(|mut group| {
            let count = group.members.len();
            group.all_members_match = group.pair_indices.len() == count * (count - 1) / 2;
            group
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::{
        dryer::SimilarityValues,
        quality::{FunctionKind, SourceLocation},
    };
    use crate::{
        dryer::{ClonePair, groups::group_pairs},
        quality::FunctionLocation,
    };

    fn member(file: &str, start: usize) -> FunctionLocation {
        FunctionLocation {
            name: "sameName".into(),
            kind: FunctionKind::FunctionDeclaration,
            location: SourceLocation {
                file: file.into(),
                start,
                end: start + 10,
                line: 1,
                end_line: 1,
            },
            name_truncated: true,
        }
    }
    fn pair(left: &FunctionLocation, right: &FunctionLocation) -> ClonePair {
        ClonePair {
            left: left.clone(),
            right: right.clone(),
            similarity: SimilarityValues {
                set: 0.9,
                multiset: 0.9,
                weighted: 0.9,
            },
            exact_normalized_match: false,
            left_opaque_nodes: 0,
            right_opaque_nodes: 0,
        }
    }
    #[test]
    fn components_retain_edges_without_assuming_similarity_is_transitive() {
        let a = member("a.ts", 10);
        let b = member("a.ts", 30);
        let c = member("b.ts", 10);
        let d = member("d.ts", 10);
        let e = member("e.ts", 10);
        let pairs = vec![pair(&d, &e), pair(&b, &c), pair(&a, &b)];
        let groups = group_pairs(&pairs);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].members, vec![a.clone(), b.clone(), c.clone()]);
        assert_eq!(groups[0].pair_indices, vec![1, 2]);
        assert!(!groups[0].all_members_match);
        assert_eq!(groups[1].members, vec![d, e]);
        assert_eq!(groups[1].pair_indices, vec![0]);
        assert!(groups[1].all_members_match);
        let mut complete = pairs;
        complete.push(pair(&a, &c));
        assert!(group_pairs(&complete)[0].all_members_match);
        complete.reverse();
        let reordered = group_pairs(&complete);
        assert_eq!(reordered[0].members, groups[0].members);
        assert_eq!(reordered[0].pair_indices, vec![0, 1, 2]);
        assert!(group_pairs(&[]).is_empty());
    }
}
