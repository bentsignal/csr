//! Navigation meshes (`maps/<map>.nav`) and path-finding.
//!
//! A nav mesh is a set of axis-aligned rectangular walkable areas with
//! directed cardinal connections and authored ladder departures. The layout follows the public
//! description of the NAV format (version 16 as shipped with CS:GO), with
//! the CS:GO-specific per-area trailer (a count byte plus 14-byte records)
//! skipped.
//! Ladder layout is grounded in public NAV format descriptions and read-only
//! Legacy v16/subversion-1 data; no game/SDK implementation is used.

use glam::Vec3;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::io;

const MAGIC: u32 = 0xFEED_FACE;

/// `NavAttributeType` bits used by bots.
pub mod attr {
    pub const CROUCH: u32 = 0x0001;
    pub const JUMP: u32 = 0x0002;
    pub const PRECISE: u32 = 0x0004;
    pub const NO_JUMP: u32 = 0x0008;
    pub const STOP: u32 = 0x0010;
    pub const RUN: u32 = 0x0020;
    pub const WALK: u32 = 0x0040;
    pub const AVOID: u32 = 0x0080;
    pub const STAIRS: u32 = 0x1000;
}

/// Connection directions: north (-Y), east (+X), south (+Y), west (-X).
pub const DIRECTIONS: usize = 4;

#[derive(Clone, Debug)]
pub struct Area {
    pub id: u32,
    pub flags: u32,
    /// North-west corner (min X, min Y).
    pub nw: Vec3,
    /// South-east corner (max X, max Y).
    pub se: Vec3,
    pub ne_z: f32,
    pub sw_z: f32,
    pub connections: [Vec<u32>; DIRECTIONS],
    pub place: u16,
    pub hiding_spots: Vec<Vec3>,
}

impl Area {
    pub fn center(&self) -> Vec3 {
        Vec3::new(
            (self.nw.x + self.se.x) * 0.5,
            (self.nw.y + self.se.y) * 0.5,
            (self.nw.z + self.se.z + self.ne_z + self.sw_z) * 0.25,
        )
    }

    pub fn contains_xy(&self, p: Vec3, margin: f32) -> bool {
        p.x >= self.nw.x - margin && p.x <= self.se.x + margin && p.y >= self.nw.y - margin && p.y <= self.se.y + margin
    }

    /// Ground height at an XY position (bilinear over the four corners).
    pub fn z_at(&self, x: f32, y: f32) -> f32 {
        let w = (self.se.x - self.nw.x).max(1e-3);
        let h = (self.se.y - self.nw.y).max(1e-3);
        let u = ((x - self.nw.x) / w).clamp(0.0, 1.0);
        let v = ((y - self.nw.y) / h).clamp(0.0, 1.0);
        // Corners: nw (0,0), ne (1,0), sw (0,1), se (1,1).
        let north = self.nw.z + (self.ne_z - self.nw.z) * u;
        let south = self.sw_z + (self.se.z - self.sw_z) * u;
        north + (south - north) * v
    }

    pub fn size(&self) -> f32 {
        (self.se.x - self.nw.x) * (self.se.y - self.nw.y)
    }
}

/// Authored NAV ladder record. Endpoints guide input, never player placement.
#[derive(Clone, Debug)]
pub struct Ladder {
    pub id: u32,
    pub width: f32,
    pub top: Vec3,
    pub bottom: Vec3,
    pub length: f32,
    /// Outward direction: north, east, south, west, like area connections.
    pub direction: u32,
    /// Forward, left, right, behind. Behind is retained for descending only.
    pub top_areas: [u32; 4],
    pub bottom_area: u32,
}

impl Ladder {
    pub fn normal(&self) -> Vec3 {
        match self.direction {
            0 => -Vec3::Y,
            1 => Vec3::X,
            2 => Vec3::Y,
            3 => -Vec3::X,
            _ => Vec3::ZERO,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct NavMesh {
    pub version: u32,
    pub places: Vec<String>,
    pub areas: Vec<Area>,
    index: HashMap<u32, usize>,
    ladders: Vec<Ladder>,
    ladder_index: HashMap<u32, usize>,
    // Kept beside Area so runtime/voxel and existing Area constructors stay valid.
    // Arrays are directed departures: up, down (not reciprocal adjacency).
    ladder_connections: HashMap<u32, [Vec<u32>; 2]>,
}

fn invalid(msg: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

struct Reader<'a> {
    d: &'a [u8],
    p: usize,
}

impl Reader<'_> {
    fn take(&mut self, n: usize) -> io::Result<&[u8]> {
        let end = self.p.checked_add(n).ok_or_else(|| invalid("nav offset overflow"))?;
        let s = self.d.get(self.p..end).ok_or_else(|| invalid(format!("nav truncated at {}", self.p)))?;
        self.p = end;
        Ok(s)
    }
    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> io::Result<u16> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> io::Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn f32(&mut self) -> io::Result<f32> {
        Ok(f32::from_bits(self.u32()?))
    }
    fn vec3(&mut self) -> io::Result<Vec3> {
        Ok(Vec3::new(self.f32()?, self.f32()?, self.f32()?))
    }
    fn count(&mut self, max: u32) -> io::Result<usize> {
        let n = self.u32()?;
        if n > max {
            return Err(invalid(format!("implausible count {n} at {}", self.p - 4)));
        }
        Ok(n as usize)
    }
}

impl NavMesh {
    /// Build runtime navigation (e.g. a voxel world whose floor can change).
    pub fn from_areas(areas: Vec<Area>) -> Self {
        let index=areas.iter().enumerate().map(|(i,a)|(a.id,i)).collect();
        Self{version:16,places:vec!["Overworld".into()],areas,index,..Default::default()}
    }
    pub fn parse(bytes: &[u8]) -> io::Result<NavMesh> {
        let mut r = Reader { d: bytes, p: 0 };
        if r.u32()? != MAGIC {
            return Err(invalid("not a nav file"));
        }
        let version = r.u32()?;
        if !(6..=16).contains(&version) {
            return Err(invalid(format!("unsupported nav version {version}")));
        }
        let subversion = if version >= 10 { r.u32()? } else { 0 };
        if version >= 4 {
            r.u32()?; // BSP size the mesh was built for
        }
        if version >= 14 {
            r.u8()?; // analysed
        }
        let mut places = Vec::new();
        if version >= 5 {
            let n = r.u16()?;
            for _ in 0..n {
                let len = r.u16()? as usize;
                let raw = r.take(len)?;
                places.push(String::from_utf8_lossy(raw).trim_end_matches('\0').to_string());
            }
            if version > 11 {
                r.u8()?; // has unnamed areas
            }
        }
        let area_count = r.count(1_000_000)?;
        let mut areas = Vec::with_capacity(area_count);
        let mut ladder_connections = HashMap::new();
        for _ in 0..area_count {
            let id = r.u32()?;
            let flags = match version {
                ..=8 => r.u8()? as u32,
                9..=12 => r.u16()? as u32,
                _ => r.u32()?,
            };
            let nw = r.vec3()?;
            let se = r.vec3()?;
            let ne_z = r.f32()?;
            let sw_z = r.f32()?;
            let mut connections: [Vec<u32>; DIRECTIONS] = Default::default();
            for dir in connections.iter_mut() {
                let n = r.count(4096)?;
                for _ in 0..n {
                    dir.push(r.u32()?);
                }
            }
            let spots = r.u8()?;
            let mut hiding_spots = Vec::with_capacity(spots as usize);
            for _ in 0..spots {
                r.u32()?;
                hiding_spots.push(r.vec3()?);
                r.u8()?;
            }
            if version < 15 {
                let approach = r.u8()?;
                r.take(approach as usize * 14)?;
            }
            let encounters = r.count(65_536)?;
            for _ in 0..encounters {
                r.take(4 + 1 + 4 + 1)?;
                let n = r.u8()?;
                r.take(n as usize * 5)?;
            }
            let place = r.u16()?;
            let mut ladder_links: [Vec<u32>; 2] = Default::default();
            for links in &mut ladder_links {
                let n = r.count(4096)?;
                for _ in 0..n { links.push(r.u32()?); }
            }
            if ladder_links.iter().any(|links| !links.is_empty()) {
                ladder_connections.insert(id, ladder_links);
            }
            r.take(8)?; // earliest occupy time per team
            if version >= 11 {
                r.take(16)?; // light intensity per corner
            }
            if version >= 16 {
                let n = r.count(1_000_000)?;
                r.take(n * 5)?; // potentially visible areas
                r.u32()?; // inherit visibility from
            }
            if subversion > 0 {
                // CS:GO-specific trailer.
                let n = r.u8()?;
                r.take(n as usize * 14)?;
            }
            areas.push(Area {
                id,
                flags,
                nw,
                se,
                ne_z,
                sw_z,
                connections,
                place,
                hiding_spots,
            });
        }
        let index: HashMap<_, _> = areas.iter().enumerate().map(|(i, a)| (a.id, i)).collect();
        if index.len() != areas.len() { return Err(invalid("duplicate nav area id")); }
        let mut ladders = Vec::new();
        let mut ladder_index = HashMap::new();
        // Older formats retain their previous cardinal-only behavior. Ladder
        // routing is admitted only for the observed Legacy v16 layout.
        if version == 16 && subversion <= 1 {
            let count = r.count(65_536)?;
            for _ in 0..count {
                let ladder = Ladder {
                    id: r.u32()?, width: r.f32()?, top: r.vec3()?, bottom: r.vec3()?,
                    length: r.f32()?, direction: r.u32()?,
                    top_areas: [r.u32()?, r.u32()?, r.u32()?, r.u32()?],
                    bottom_area: r.u32()?,
                };
                if ladder.id == 0 || ladder_index.insert(ladder.id, ladders.len()).is_some()
                    || !ladder.top.is_finite() || !ladder.bottom.is_finite()
                    || !ladder.width.is_finite() || ladder.width <= 0.0
                    || !ladder.length.is_finite() || ladder.length <= 0.0
                    || ladder.direction >= 4 || ladder.top.z <= ladder.bottom.z
                    || !ladder.top.distance(ladder.bottom).is_finite()
                { return Err(invalid("invalid nav ladder record")); }
                ladders.push(ladder);
            }
        }
        Ok(NavMesh {
            version,
            places,
            areas,
            index,
            ladders,
            ladder_index,
            ladder_connections,
        })
    }

    pub fn area(&self, id: u32) -> Option<&Area> {
        self.index.get(&id).map(|&i| &self.areas[i])
    }

    pub fn ladders(&self) -> &[Ladder] { &self.ladders }

    pub fn ladder(&self, id: u32) -> Option<&Ladder> {
        self.ladder_index.get(&id).map(|&i| &self.ladders[i])
    }

    /// Authored departures in stable file order. Unknown/dangling IDs never
    /// create inferred edges. Down links may come from additional top areas
    /// (observed in installed Mirage/Nuke/Train/Overpass NAVs).
    fn ladder_edges(&self, from: u32) -> impl Iterator<Item = (u32, &Ladder, bool)> {
        self.ladder_connections.get(&from).into_iter().flat_map(move |links| {
            links.iter().enumerate().flat_map(move |(direction, ids)| {
                ids.iter().filter_map(move |id| self.ladder(*id)).flat_map(move |ladder| {
                    let up = direction == 0;
                    let destinations = if up && from == ladder.bottom_area {
                        [ladder.top_areas[0], ladder.top_areas[1], ladder.top_areas[2]]
                    } else if !up && from != ladder.bottom_area {
                        [ladder.bottom_area, 0, 0]
                    } else { [0; 3] };
                    destinations.into_iter().filter(move |&to| to != 0 && to != from && self.area(to).is_some())
                        .map(move |to| (to, ladder, up))
                })
            })
        })
    }

    /// Same mount chosen by routing. Existing cardinal crossings take priority;
    /// parallel ladder records use the first authored departure, never proximity.
    pub fn ladder_transition(&self, from: u32, to: u32) -> Option<(&Ladder, bool)> {
        if self.area(from)?.connections.iter().any(|dir| dir.contains(&to)) { return None; }
        self.ladder_edges(from).find(|(next, _, _)| *next == to).map(|(_, ladder, up)| (ladder, up))
    }

    pub fn place_name(&self, place: u16) -> Option<&str> {
        // Place 0 is "no place"; names are 1-based.
        place.checked_sub(1).and_then(|i| self.places.get(i as usize)).map(String::as_str)
    }

    /// The area under (or nearest to) a point.
    pub fn nearest_area(&self, p: Vec3) -> Option<&Area> {
        let mut best: Option<(&Area, f32)> = None;
        for a in &self.areas {
            let score = if a.contains_xy(p, 0.0) {
                let dz = p.z - a.z_at(p.x, p.y);
                // Prefer the floor just below the point.
                if (-8.0..=80.0).contains(&dz) { dz.abs() * 0.01 } else { dz.abs() + 1000.0 }
            } else {
                let cx = p.x.clamp(a.nw.x, a.se.x);
                let cy = p.y.clamp(a.nw.y, a.se.y);
                let cz = a.z_at(cx, cy);
                Vec3::new(cx, cy, cz).distance(p) + 100.0
            };
            if best.is_none_or(|(_, s)| score < s) {
                best = Some((a, score));
            }
        }
        best.map(|(a, _)| a)
    }

    /// A* over area centres. Returns area ids from `from` to `to` inclusive.
    pub fn find_path(&self, from: u32, to: u32) -> Option<Vec<u32>> {
        self.find_path_with_cost(from,to,|_|0.0)
    }

    /// Additional nonnegative tactical traversal cost (fire, exposed approaches).
    pub fn find_path_with_cost(&self, from: u32, to: u32, extra: impl Fn(&Area)->f32) -> Option<Vec<u32>> {
        self.find_path_with_traversal(from, to, extra, |_, _| true)
    }

    /// Tactical costs with an additional directed-transition admission rule.
    /// Rejecting one crossing does not exclude its destination from other routes.
    pub fn find_path_with_traversal(
        &self,
        from: u32,
        to: u32,
        extra: impl Fn(&Area) -> f32,
        allow_transition: impl Fn(u32, u32) -> bool,
    ) -> Option<Vec<u32>> {
        #[derive(PartialEq)]
        struct Node(f32, u32);
        impl Eq for Node {}
        impl PartialOrd for Node {
            fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for Node {
            fn cmp(&self, o: &Self) -> Ordering {
                o.0.total_cmp(&self.0)
            }
        }
        let goal = self.area(to)?.center();
        self.area(from)?;
        let mut open = BinaryHeap::new();
        let mut came: HashMap<u32, u32> = HashMap::new();
        let mut cost: HashMap<u32, f32> = HashMap::new();
        cost.insert(from, 0.0);
        open.push(Node(0.0, from));
        while let Some(Node(_, id)) = open.pop() {
            if id == to {
                let mut path = vec![to];
                let mut cur = to;
                while let Some(&prev) = came.get(&cur) {
                    path.push(prev);
                    cur = prev;
                }
                path.reverse();
                return Some(path);
            }
            let a = self.area(id)?;
            let g = cost[&id];
            let cardinal = a.connections.iter().flatten().copied().map(|next| (next, None));
            let mut departures = HashSet::new();
            if self.ladder_connections.contains_key(&id) {
                departures.extend(a.connections.iter().flatten().copied());
            }
            // First authored mount per destination, exactly as ladder_transition.
            // Do not rescan the departure arrays for every ladder/exit pair.
            let ladder_edges = self.ladder_edges(id).filter(move |(next, _, _)| departures.insert(*next))
                .map(|(next, ladder, _)| (next, Some(ladder)));
            // Cardinal iteration/order/costs stay unchanged; append authored
            // ladder departures only after the four existing directions.
            for (next, ladder) in cardinal.chain(ladder_edges) {
                if !allow_transition(id, next) {
                    continue;
                }
                let Some(b) = self.area(next) else { continue };
                let mut step = if let Some(ladder) = ladder {
                    let (entry, exit) = if id == ladder.bottom_area {
                        (ladder.bottom, ladder.top)
                    } else { (ladder.top, ladder.bottom) };
                    a.center().distance(entry) + ladder.length.max(entry.distance(exit)) + exit.distance(b.center())
                } else { a.center().distance(b.center()) };
                if !step.is_finite() { continue; }
                if b.flags & attr::AVOID != 0 {
                    step *= 4.0;
                }
                if b.flags & attr::CROUCH != 0 {
                    step *= 1.5;
                }
                let penalty=extra(b);
                let ng = g + step + if penalty.is_finite() {penalty.max(0.0)}else{0.0};
                if cost.get(&next).is_none_or(|&c| ng < c) {
                    cost.insert(next, ng);
                    came.insert(next, id);
                    open.push(Node(ng + b.center().distance(goal), next));
                }
            }
        }
        None
    }

    /// A point to aim for when crossing from area `a` into its neighbour
    /// `b`: the middle of their shared edge, pulled inside by `inset`.
    pub fn portal(&self, a: u32, b: u32, inset: f32) -> Option<Vec3> {
        let (a, b) = (self.area(a)?, self.area(b)?);
        let lo_x = a.nw.x.max(b.nw.x);
        let hi_x = a.se.x.min(b.se.x);
        let lo_y = a.nw.y.max(b.nw.y);
        let hi_y = a.se.y.min(b.se.y);
        let mid = |lo: f32, hi: f32, fallback: f32| {
            if hi >= lo {
                let m = (lo + hi) * 0.5;
                if hi - lo > 2.0 * inset { m.clamp(lo + inset, hi - inset) } else { m }
            } else {
                fallback
            }
        };
        let bc = b.center();
        let x = mid(lo_x, hi_x, bc.x);
        let y = mid(lo_y, hi_y, bc.y);
        Some(Vec3::new(x, y, b.z_at(x, y)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(id: u32, x0: f32, y0: f32, x1: f32, y1: f32, conns: [Vec<u32>; 4]) -> Area {
        Area {
            id,
            flags: 0,
            nw: Vec3::new(x0, y0, 0.0),
            se: Vec3::new(x1, y1, 0.0),
            ne_z: 0.0,
            sw_z: 0.0,
            connections: conns,
            place: 0,
            hiding_spots: Vec::new(),
        }
    }

    fn mesh(areas: Vec<Area>) -> NavMesh {
        let index = areas.iter().enumerate().map(|(i, a)| (a.id, i)).collect();
        NavMesh {
            version: 16,
            places: vec![],
            areas,
            index,
            ..Default::default()
        }
    }

    #[test]
    fn path_goes_around() {
        // 1 → 2 → 3 in an L; 1 and 3 are not directly connected.
        let m = mesh(vec![
            area(1, 0.0, 0.0, 100.0, 100.0, [vec![], vec![2], vec![], vec![]]),
            area(2, 100.0, 0.0, 200.0, 100.0, [vec![], vec![], vec![3], vec![1]]),
            area(3, 100.0, 100.0, 200.0, 200.0, [vec![2], vec![], vec![], vec![]]),
        ]);
        assert_eq!(m.find_path(1, 3), Some(vec![1, 2, 3]));
        assert_eq!(m.find_path(3, 1), Some(vec![3, 2, 1]));
        let p = m.portal(1, 2, 16.0).unwrap();
        assert!((p.x - 100.0).abs() < 1e-3 && (p.y - 50.0).abs() < 1e-3);
        assert_eq!(m.nearest_area(Vec3::new(150.0, 150.0, 10.0)).unwrap().id, 3);
    }

    // Own synthetic v16/subversion-1 bytes, not installed game data. UNRUN in
    // the source-only implementation slot. Exercise the actual file parser.
    fn ladder_file(up: &[u32], down: &[u32]) -> Vec<u8> {
        let mut bytes = Vec::new();
        let word = |bytes: &mut Vec<u8>, value: u32| bytes.extend(value.to_le_bytes());
        for value in [MAGIC, 16, 1, 0] { word(&mut bytes, value); }
        bytes.push(1); // analysed
        bytes.extend(0u16.to_le_bytes()); bytes.push(1); // no named places
        word(&mut bytes, 5);
        for id in 1..=5 {
            word(&mut bytes, id); word(&mut bytes, 0);
            let z: f32 = if id == 1 { 0.0 } else { 256.0 };
            for value in [-128.0f32, -64.0, z, 128.0, 64.0, z, z, z] {
                word(&mut bytes, value.to_bits());
            }
            for _ in 0..4 { word(&mut bytes, 0); } // cardinal lists
            bytes.push(0); word(&mut bytes, 0); // hiding/encounters
            bytes.extend(0u16.to_le_bytes());
            let departures = if id == 1 { [up, &[][..]] } else { [&[][..], down] };
            for ids in departures {
                word(&mut bytes, ids.len() as u32);
                for &ladder in ids { word(&mut bytes, ladder); }
            }
            bytes.extend([0; 24]); // occupy/light
            word(&mut bytes, 0); word(&mut bytes, 0); // visibility
            bytes.push(0); // CS:GO trailer
        }
        word(&mut bytes, 1);
        word(&mut bytes, 7); word(&mut bytes, 48f32.to_bits());
        for value in [0.0f32, 0.0, 256.0, 0.0, 0.0, 0.0, 256.0] {
            word(&mut bytes, value.to_bits());
        }
        for value in [3, 2, 3, 0, 4, 1] { word(&mut bytes, value); }
        bytes
    }

    #[test]
    fn parsed_ladders_preserve_directed_departures_and_canonical_mount() {
        let mut m = NavMesh::parse(&ladder_file(&[7], &[7])).unwrap();
        assert_eq!(m.ladders().len(), 1);
        assert_eq!(m.ladder(7).unwrap().normal(), -Vec3::X);
        assert_eq!(m.find_path(1, 2), Some(vec![1, 2]));
        assert_eq!(m.find_path(1, 3), Some(vec![1, 3]));
        assert_eq!(m.find_path(1, 4), None, "behind slot is not an ascending exit");
        for from in [2, 3, 4, 5] {
            assert_eq!(m.find_path(from, 1), Some(vec![from, 1]), "additional authored down links are valid");
            assert!(!m.ladder_transition(from, 1).unwrap().1);
        }
        assert_eq!(m.find_path_with_traversal(1, 2, |_| 0.0, |_, _| false), None);
        assert_eq!(NavMesh::parse(&ladder_file(&[7], &[])).unwrap().find_path(2, 1), None);
        assert_eq!(NavMesh::parse(&ladder_file(&[999], &[999])).unwrap().find_path(1, 2), None);
        // A second, cheaper ladder must not silently change mount identity in
        // an area-id path. Both planner and follower select the first departure.
        let mut other = m.ladders[0].clone(); other.id = 8; other.length = 1.0;
        m.ladder_index.insert(8, 1); m.ladders.push(other);
        m.ladder_connections.get_mut(&1).unwrap()[0].push(8);
        assert_eq!(m.ladder_transition(1, 2).unwrap().0.id, 7);
        m.areas[0].connections[1].push(2);
        assert!(m.ladder_transition(1, 2).is_none(), "cardinal crossing retains priority");
        assert_eq!(m.find_path(1, 2), Some(vec![1, 2]));
    }

    #[test]
    fn malformed_ladder_records_fail_closed_and_dangling_targets_do_not_route() {
        let bytes = ladder_file(&[7], &[7]);
        let record = bytes.len() - 60;
        for end in record - 4..bytes.len() {
            assert!(NavMesh::parse(&bytes[..end]).is_err(), "truncation at {end}");
        }
        for (offset, value) in [(0, 0), (4, f32::NAN.to_bits()), (4, 0),
            (8, f32::INFINITY.to_bits()), (28, 256f32.to_bits()), (32, (-1f32).to_bits()), (36, 4)] {
            let mut bad = bytes.clone(); bad[record + offset..record + offset + 4].copy_from_slice(&value.to_le_bytes());
            assert!(NavMesh::parse(&bad).is_err(), "invalid ladder field at {offset}");
        }
        let mut bad = bytes.clone(); bad[record - 4..record].copy_from_slice(&65_537u32.to_le_bytes());
        assert!(NavMesh::parse(&bad).is_err());
        let mut duplicate = bytes.clone(); duplicate[record - 4..record].copy_from_slice(&2u32.to_le_bytes());
        duplicate.extend_from_slice(&bytes[record..]);
        assert!(NavMesh::parse(&duplicate).is_err());
        let mut dangling = bytes.clone();
        dangling[record + 56..record + 60].copy_from_slice(&999u32.to_le_bytes());
        let m = NavMesh::parse(&dangling).unwrap();
        assert_eq!(m.find_path(2, 1), None);
        assert_eq!(m.find_path(1, 2), None, "up source must match the authored bottom");
    }
}
