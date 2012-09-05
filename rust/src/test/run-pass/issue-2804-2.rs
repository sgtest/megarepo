// Minimized version of issue-2804.rs. Both check that callee IDs don't
// clobber the previous node ID in a macro expr
use std;
use std::map::hashmap;

fn add_interfaces(managed_ip: ~str, device: std::map::hashmap<~str, int>)  {
     error!("%s, %?", managed_ip, device[~"interfaces"]);
}

fn main() {}
