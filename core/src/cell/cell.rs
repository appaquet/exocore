use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, RwLock},
};

use exocore_protos::{
    apps::Manifest,
    generated::exocore_core::{CellConfig, LocalNodeConfig},
    registry::Registry,
};
use libp2p::PeerId;

use super::{
    cell_apps::cell_app_directory, config::CellConfigExt, ApplicationId, CellApplications,
    CellNode, CellNodeRole, CellNodes, CellNodesRead, CellNodesWrite, Error, LocalNode, Node,
    NodeId,
};
use crate::{
    dir::DynDirectory,
    sec::keys::{Keypair, PublicKey},
};

const CELL_CONFIG_FILE: &str = "cell.yaml";

/// A Cell represents a private enclosure in which the data and applications of
/// a user are hosted. A Cell resides on multiple nodes.
#[derive(Clone)]
pub struct Cell {
    identity: Arc<Identity>,
    nodes: Arc<RwLock<HashMap<NodeId, CellNode>>>,
    apps: CellApplications,
    schemas: Arc<Registry>,
    dir: Option<DynDirectory>,
}

struct Identity {
    public_key: PublicKey,
    cell_id: CellId,
    local_node: LocalNode,
    name: String,
}

impl Cell {
    pub fn from_config(config: CellConfig, local_node: LocalNode) -> Result<EitherCell, Error> {
        let either_cell = if !config.keypair.is_empty() {
            let keypair = Keypair::decode_base58_string(&config.keypair)
                .map_err(|err| Error::Cell(anyhow!("Couldn't parse cell keypair: {}", err)))?;

            let name = Some(config.name.clone()).filter(String::is_empty);

            let full_cell = FullCell::build(keypair, local_node, name);
            EitherCell::Full(Box::new(full_cell))
        } else {
            let public_key = PublicKey::decode_base58_string(&config.public_key)
                .map_err(|err| Error::Cell(anyhow!("Couldn't parse cell public key: {}", err)))?;

            let name = Some(config.name.clone()).filter(String::is_empty);

            let cell = Cell::build(public_key, local_node, name);
            EitherCell::Cell(Box::new(cell))
        };

        {
            // load nodes from config
            let mut nodes = either_cell.nodes_mut();
            for node_config in &config.nodes {
                let node = Node::from_config(node_config.node.clone().ok_or_else(|| {
                    Error::Config(anyhow!("Cell node config node is not defined"))
                })?)?;

                let mut cell_node = CellNode::new(node);

                for role in node_config.roles() {
                    cell_node.add_role(CellNodeRole::from_config(role)?);
                }

                nodes.add_cell_node(cell_node);
            }
        }

        {
            // load apps from config
            let cell = either_cell.cell();
            if let Ok(apps_dir) = cell.apps_directory() {
                cell.apps
                    .load_from_cell_apps_conf(apps_dir, config.apps.iter())?;
            }
        }

        Ok(either_cell)
    }

    pub fn from_directory(
        dir: impl Into<DynDirectory>,
        local_node: LocalNode,
    ) -> Result<EitherCell, Error> {
        let dir = dir.into();

        let cell_config = {
            let config_file = dir.open_read(Path::new(CELL_CONFIG_FILE))?;
            CellConfig::from_yaml(config_file)?
        };

        Self::from_config(cell_config, local_node)
    }

    // TODO: Remove me
    #[deprecated]
    pub fn from_local_node_old(
        local_node: LocalNode,
    ) -> Result<(Vec<EitherCell>, LocalNode), Error> {
        let config = local_node.config();

        let mut either_cells = Vec::new();
        for node_cell_config in &config.cells {
            let cell_config = CellConfig::from_node_cell(node_cell_config)?;
            let either_cell = Self::from_config(cell_config, local_node.clone())?;
            either_cells.push(either_cell);
        }

        Ok((either_cells, local_node))
    }

    pub fn from_local_node(local_node: LocalNode) -> Result<(Vec<EitherCell>, LocalNode), Error> {
        let config = local_node.config();

        let mut either_cells = Vec::new();
        for node_cell_config in &config.cells {
            let cell_id = CellId::from_str(&node_cell_config.id).map_err(|_err| {
                Error::Cell(anyhow!("couldn't parse cell id '{}'", node_cell_config.id))
            })?;

            let cell_dir = local_node.cell_directory(&cell_id)?;
            let either_cell =
                Cell::from_directory(cell_dir, local_node.clone()).map_err(|err| {
                    Error::Cell(anyhow!("Failed to load cell id '{}': {}", cell_id, err))
                })?;
            either_cells.push(either_cell);
        }

        Ok((either_cells, local_node))
    }

    // TODO: Remove
    #[deprecated]
    pub fn from_local_node_config(
        config: LocalNodeConfig,
    ) -> Result<(Vec<EitherCell>, LocalNode), Error> {
        let local_node = LocalNode::from_config(config)?;
        Self::from_local_node_old(local_node)
    }

    pub fn from_local_node_directory(
        dir: impl Into<DynDirectory>,
    ) -> Result<(Vec<EitherCell>, LocalNode), Error> {
        let local_node = LocalNode::from_directory(dir.into())?;
        Self::from_local_node(local_node)
    }

    fn build(public_key: PublicKey, local_node: LocalNode, name: Option<String>) -> Cell {
        let cell_id = CellId::from_public_key(&public_key);

        let mut nodes_map = HashMap::new();
        let local_cell_node = CellNode::new(local_node.node().clone());
        nodes_map.insert(local_node.id().clone(), local_cell_node);

        let name = name.unwrap_or_else(|| public_key.generate_name());

        let schemas = Arc::new(Registry::new_with_exocore_types());

        let dir = local_node.cell_directory(&cell_id).ok();

        Cell {
            identity: Arc::new(Identity {
                public_key,
                cell_id,
                local_node,
                name,
            }),
            apps: CellApplications::new(schemas.clone()),
            nodes: Arc::new(RwLock::new(nodes_map)),
            schemas,
            dir,
        }
    }

    pub fn id(&self) -> &CellId {
        &self.identity.cell_id
    }

    pub fn name(&self) -> &str {
        &self.identity.name
    }

    pub fn local_node(&self) -> &LocalNode {
        &self.identity.local_node
    }

    pub fn local_node_has_role(&self, role: CellNodeRole) -> bool {
        let nodes = self.nodes();
        if let Some(cn) = nodes.get(self.identity.local_node.id()) {
            cn.has_role(role)
        } else {
            false
        }
    }

    pub fn public_key(&self) -> &PublicKey {
        &self.identity.public_key
    }

    pub fn nodes(&self) -> CellNodesRead {
        let nodes = self
            .nodes
            .read()
            .expect("Couldn't acquire read lock on nodes");
        CellNodesRead { cell: self, nodes }
    }

    pub fn nodes_mut(&self) -> CellNodesWrite {
        let nodes = self
            .nodes
            .write()
            .expect("Couldn't acquire write lock on nodes");
        CellNodesWrite { cell: self, nodes }
    }

    pub fn schemas(&self) -> &Arc<Registry> {
        &self.schemas
    }

    pub fn applications(&self) -> &CellApplications {
        &self.apps
    }

    pub fn directory(&self) -> Option<&DynDirectory> {
        self.dir.as_ref()
    }

    pub fn cell_directory(&self) -> Option<PathBuf> {
        let dir = self.directory()?;
        let path = dir.as_os_path().expect("couldn't get os path for cell");
        Some(path)
    }

    #[deprecated]
    pub fn chain_directory(&self) -> Option<PathBuf> {
        self.cell_directory().map(|mut dir| {
            dir.push("chain");
            dir
        })
    }

    #[deprecated]
    pub fn store_directory(&self) -> Option<PathBuf> {
        self.cell_directory().map(|mut dir| {
            dir.push("store");
            dir
        })
    }

    pub fn apps_directory(&self) -> Result<DynDirectory, Error> {
        let dir = self.directory().ok_or(Error::NoDirectory)?;
        let apps_dir = dir.scope(Path::new("apps").to_path_buf())?;
        Ok(apps_dir)
    }

    pub fn app_directory(&self, app_manifest: &Manifest) -> Result<DynDirectory, Error> {
        let app_id = ApplicationId::from_base58_public_key(&app_manifest.public_key)?;
        let apps_dir = self.apps_directory()?;
        cell_app_directory(&apps_dir, &app_id, &app_manifest.version)
    }

    pub fn temp_directory(&self) -> Option<PathBuf> {
        self.cell_directory().map(|dir| {
            let mut dir = dir;
            dir.push("tmp");

            dir
        })
    }
}

impl std::fmt::Display for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("Cell{")?;
        f.write_str(&self.identity.name)?;
        f.write_str("}")
    }
}

/// Unique identifier of a cell, which is built by hashing the public key
///
/// For now, this ID is generated the same way as node IDs.
#[derive(PartialEq, Eq, Clone, Debug, Hash)]
pub struct CellId(String);

impl CellId {
    /// Create a Cell ID from a public key by using libp2p method to be
    /// compatible with it
    pub fn from_public_key(public_key: &PublicKey) -> CellId {
        let peer_id = PeerId::from_public_key(public_key.to_libp2p().clone());
        CellId(peer_id.to_string())
    }

    pub fn from_string(id: String) -> CellId {
        CellId(id)
    }

    pub fn from_bytes(id: &[u8]) -> CellId {
        CellId(String::from_utf8_lossy(id).to_string())
    }

    pub fn as_bytes(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

impl std::fmt::Display for CellId {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::str::FromStr for CellId {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(CellId(s.to_string()))
    }
}

/// A Cell for which we have full access since we have the private key.
#[derive(Clone)]
pub struct FullCell {
    cell: Cell,
    keypair: Keypair,
}

impl FullCell {
    pub fn from_keypair(keypair: Keypair, local_node: LocalNode) -> FullCell {
        Self::build(keypair, local_node, None)
    }

    pub fn generate_old(local_node: LocalNode) -> FullCell {
        let cell_keypair = Keypair::generate_ed25519();
        Self::build(cell_keypair, local_node, None)
    }

    pub fn generate(local_node: LocalNode) -> Result<FullCell, Error> {
        let cell_keypair = Keypair::generate_ed25519();

        let full_cell = Self::build(cell_keypair, local_node, None);
        full_cell.save_config()?;

        Ok(full_cell)
    }

    fn build(keypair: Keypair, local_node: LocalNode, name: Option<String>) -> FullCell {
        FullCell {
            cell: Cell::build(keypair.public(), local_node, name),
            keypair,
        }
    }

    pub fn keypair(&self) -> &Keypair {
        &self.keypair
    }

    pub fn cell(&self) -> &Cell {
        &self.cell
    }

    pub fn generate_config(&self, full: bool) -> CellConfig {
        let mut cell_config = CellConfig {
            public_key: self.cell.public_key().encode_base58_string(),
            id: self.cell.id().to_string(),
            name: self.cell.name().to_string(),
            ..Default::default()
        };

        if full {
            cell_config.keypair = self.keypair.encode_base58_string();
        }

        {
            let nodes = self.cell.nodes();
            for node in nodes.iter().all() {
                cell_config.nodes.push(node.to_config());
            }
        }

        cell_config
    }

    pub fn save_config(&self) -> Result<(), Error> {
        let dir = self.cell.dir.as_ref().ok_or(Error::NoDirectory)?;
        let file = dir.open_write(Path::new(CELL_CONFIG_FILE))?;
        let config = self.generate_config(true);
        config.to_yaml_writer(file)?;
        Ok(())
    }

    #[cfg(any(test, feature = "tests-utils"))]
    pub fn with_local_node(self, local_node: LocalNode) -> FullCell {
        FullCell::from_keypair(self.keypair, local_node)
    }
}

/// Enum wrapping a full or non-full cell
#[derive(Clone)]
pub enum EitherCell {
    Full(Box<FullCell>),
    Cell(Box<Cell>),
}

impl EitherCell {
    pub fn nodes(&self) -> CellNodesRead {
        match self {
            EitherCell::Full(full_cell) => full_cell.cell().nodes(),
            EitherCell::Cell(cell) => cell.nodes(),
        }
    }

    pub fn nodes_mut(&self) -> CellNodesWrite {
        match self {
            EitherCell::Full(full_cell) => full_cell.cell().nodes_mut(),
            EitherCell::Cell(cell) => cell.nodes_mut(),
        }
    }

    pub fn cell(&self) -> &Cell {
        match self {
            EitherCell::Full(cell) => cell.cell(),
            EitherCell::Cell(cell) => cell,
        }
    }

    pub fn unwrap_full(self) -> FullCell {
        match self {
            EitherCell::Full(cell) => cell.as_ref().clone(),
            _ => panic!("Tried to unwrap EitherCell into Full, but wasn't"),
        }
    }

    pub fn unwrap_cell(self) -> Cell {
        match self {
            EitherCell::Cell(cell) => cell.as_ref().clone(),
            _ => panic!("Tried to unwrap EitherCell into Cell, but wasn't"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dir::{ram::RamDirectory, Directory};

    #[test]
    fn test_save_load_directory() {
        let dir = RamDirectory::new();
        let node = LocalNode::generate_in_directory(dir.clone()).unwrap();

        let cell1 = FullCell::generate(node.clone()).unwrap();
        let cell_dir = cell1.cell().directory().unwrap().clone();

        let cell2 = Cell::from_directory(cell_dir, node).unwrap();
        assert_eq!(cell1.cell().id(), cell2.cell().id());
    }
}
