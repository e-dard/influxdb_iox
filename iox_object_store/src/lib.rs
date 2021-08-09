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
use futures::{stream::BoxStream, Stream, StreamExt, TryStreamExt};
use object_store::{
    path::{parsed::DirsAndFileName, ObjectStorePath, Path},
    ObjectStore, ObjectStoreApi, Result,
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

    /// The name of the database this object store is for.
    pub fn database_name(&self) -> &str {
        &self.database_name
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

    /// Store this data in this database's object store.
    pub async fn put<S>(
        &self,
        location: &RelativePath,
        bytes: S,
        length: Option<usize>,
    ) -> Result<()>
    where
        S: Stream<Item = io::Result<Bytes>> + Send + Sync + 'static,
    {
        let path = self.root_path.join(location);
        self.store.put(&path, bytes, length).await
    }

    /// List the relative paths in this database's object store.
    pub async fn list(
        &self,
        _prefix: Option<&RelativePath>,
    ) -> Result<BoxStream<'static, Result<Vec<RelativePath>>>> {
        unimplemented!()
        // let path = prefix.map(|p| self.root_path.join(p));
        // let store = Arc::clone(&self.store);
        // let root_path = self.root_path.clone();
        // Ok(store
        //     .list(path.as_ref())
        //     .await
        //     .map(move |stream| {
        //         stream.map_ok(move |list| {
        //             list.into_iter()
        //                 .map(|list_item| root_path.relative(list_item))
        //                 .collect()
        //         })
        //     })?
        //     .boxed())
    }

    /// List all the catalog transaction files in object storage for this database.
    pub async fn catalog_transactions(
        &self,
    ) -> Result<BoxStream<'static, Result<Vec<Transaction>>>> {
        Ok(self.list(Some(&RelativePath {
            parts: vec!["transactions".into()],
        }))
        .await?
        .map_ok(|paths| paths.into_iter().map(Transaction::new).collect::<Vec<_>>())
        .boxed())
    }

    // pub async fn list_with_delimiter(
    //     &self,
    //     prefix: &RelativePath,
    // ) -> Result<ListResult<RelativePath>> {
    //     let path = self.root_path.join(prefix);
    //     self.store.list_with_delimiter(&path).await.map(|list| {
    //
    //     })
    // }

    /// Get the data in this relative path in this database's object store.
    pub async fn get(&self, location: &RelativePath) -> Result<BoxStream<'static, Result<Bytes>>> {
        let path = self.root_path.join(location);
        self.store.get(&path).await
    }

    // pub async fn delete(&self, location: &RelativePath) -> Result<()> {
    //     let path = self.root_path.join(location);
    //     self.store.delete(&path).await
    // }
}

/// A database-specific object store path that all `RelativePath`s should be within.
#[derive(Debug, Clone)]
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

    fn relative(&self, full_path: Path) -> RelativePath {
        let parsed_root: DirsAndFileName = self.root.clone().into();
        let parsed_full: DirsAndFileName = full_path.into();

        RelativePath {
            parts: parsed_full
                .parts_after_prefix(&parsed_root)
                .expect("Full path should have started with the root")
                .into_iter()
                .map(|part| part.to_string())
                .collect(),
        }
    }
}

/// A path within a database's object store directory. Must be combined with a database root path
/// to get an object store path.
#[derive(Debug)]
pub struct RelativePath {
    parts: Vec<String>,
}

#[derive(Debug)]
pub struct Transaction {
    relative_path: RelativePath,
}

impl Transaction {
    fn new(relative_path: RelativePath) -> Self {
        Self { relative_path }
    }
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
