#![deny(broken_intra_doc_links, rustdoc::bare_urls, rust_2018_idioms)]
#![warn(
    missing_copy_implementations,
    missing_debug_implementations,
    missing_docs,
    clippy::explicit_iter_loop,
    clippy::future_not_send,
    clippy::use_self,
    clippy::clone_on_ref_ptr
)]

//! Wraps the object_store crate with IOx-specific semantics.

use bytes::Bytes;
use data_types::{server_id::ServerId, DatabaseName};
use futures::{stream::BoxStream, Stream};
use object_store::{
    path::{parsed::DirsAndFileName, ObjectStorePath, Path},
    ListResult, ObjectStore, ObjectStoreApi, Result,
};
use std::{io, sync::Arc};

/// Handles persistence of data for a particular database. Writes within its directory/prefix.
#[derive(Debug)]
pub struct IoxObjectStore {
    store: Arc<ObjectStore>,
    server_id: ServerId,
    database_name: String, // data_types DatabaseName?
    root_path: RootPath,
}

impl IoxObjectStore {
    /// Create a database-specific wrapper. Takes all the information needed to create the
    /// root directory of a database.
    pub fn new(
        store: Arc<ObjectStore>,
        server_id: ServerId,
        database_name: &DatabaseName<'_>,
    ) -> Self {
        let root_path = RootPath::new(store.new_path(), server_id, database_name);
        Self {
            store,
            server_id,
            database_name: database_name.into(),
            root_path,
        }
    }

    pub fn database_name(&self) -> &str {
        &self.database_name
    }

    /// Path where transactions are stored.
    ///
    /// The format is:
    ///
    /// ```text
    /// <server_id>/<db_name>/transactions/
    /// ```
    pub fn catalog_path(&self) -> Path {
        let mut path = self.store.new_path();
        path.push_dir(self.server_id.to_string());
        path.push_dir(&self.database_name);
        path.push_dir("transactions");
        path
    }

    /// Location where parquet data goes to.
    ///
    /// Schema currently is:
    ///
    /// ```text
    /// <server_id>/<db_name>/data/
    /// ```
    pub fn data_path(&self) -> Path {
        let mut path = self.store.new_path();
        path.push_dir(self.server_id.to_string());
        path.push_dir(&self.database_name);
        path.push_dir("data");
        path
    }

    pub async fn put<S>(&self, _location: &Path, _bytes: S, _length: Option<usize>) -> Result<()>
    where
        S: Stream<Item = io::Result<Bytes>> + Send + Sync + 'static,
    {
        unimplemented!();
    }

    pub async fn list<'a>(
        &'a self,
        _prefix: Option<&'a Path>,
    ) -> Result<BoxStream<'a, Result<Vec<Path>>>> {
        unimplemented!();
    }

    pub async fn list_with_delimiter(&self, _prefix: &Path) -> Result<ListResult<Path>> {
        unimplemented!();
    }

    pub async fn get(&self, _location: &Path) -> Result<BoxStream<'static, Result<Bytes>>> {
        unimplemented!();
    }

    pub async fn delete(&self, _location: &Path) -> Result<()> {
        unimplemented!();
    }

    pub fn path_from_dirs_and_filename(&self, _path: DirsAndFileName) -> Path {
        unimplemented!();
    }
}

/// A database-specific object store path that all `RelativePath`s should be within.
#[derive(Debug)]
struct RootPath {
    root: Path,
}

impl RootPath {
    /// How the root of a database is defined in object storage.
    fn new(mut root: Path, server_id: ServerId, database_name: &DatabaseName<'_>) -> Self {
        root.push_dir(server_id.to_string());
        root.push_dir(database_name.as_str());
        Self { root }
    }

    /// Create an object storage path relative to `self` with the given relative path.
    fn join(&self, relative: &RelativePath) -> Path {
        let mut path = self.root.clone();

        for part in &relative.parts {
            path.push_dir(part);
        }

        path
    }
}

/// A path within a database's object store directory. Must be combined with a database root path
/// to get an object store path.
#[derive(Debug)]
pub struct RelativePath {
    parts: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::num::NonZeroU32;

    /// Creates new test server ID
    fn make_server_id() -> ServerId {
        ServerId::new(NonZeroU32::new(1).unwrap())
    }

    /// Creates a new in-memory object store. These tests rely on the `Path`s being of type
    /// `DirsAndFileName` and thus using object_store::path::DELIMITER as the separator
    fn make_object_store() -> Arc<ObjectStore> {
        Arc::new(ObjectStore::new_in_memory())
    }

    #[test]
    fn catalog_path_is_relative_to_db_root() {
        let server_id = make_server_id();
        let database_name = DatabaseName::new("clouds").unwrap();
        let iox_object_store = IoxObjectStore::new(make_object_store(), server_id, &database_name);
        assert_eq!(
            iox_object_store.catalog_path().display(),
            "1/clouds/transactions/"
        );
    }

    #[test]
    fn data_path_is_relative_to_db_root() {
        let server_id = make_server_id();
        let database_name = DatabaseName::new("clouds").unwrap();
        let iox_object_store = IoxObjectStore::new(make_object_store(), server_id, &database_name);
        assert_eq!(iox_object_store.data_path().display(), "1/clouds/data/");
    }

    #[test]
    fn root_path_adds_itself_to_all_object_store_paths() {
        let object_store = make_object_store();
        let server_id = make_server_id();
        let database_name = DatabaseName::new("clouds").unwrap();
        let root = RootPath::new(object_store.new_path(), server_id, &database_name);

        let relative = RelativePath {
            parts: vec![String::from("foo"), String::from("bar")],
        };

        let mut expected = object_store.new_path();
        expected.push_dir(server_id.to_string());
        expected.push_dir(&database_name);
        expected.push_dir("foo");
        expected.push_dir("bar");

        assert_eq!(expected, root.join(&relative));
    }
}
