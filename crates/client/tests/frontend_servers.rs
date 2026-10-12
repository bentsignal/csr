// Compile and test the leaf modules before the application wiring lands.
#[path = "../src/frontend_servers.rs"]
mod frontend_servers;
#[path = "../src/frontend_servers_data.rs"]
mod frontend_servers_data;
