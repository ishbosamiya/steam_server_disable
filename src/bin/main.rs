use steam_server_disable::ServerObject;

fn main() {
    let obj = ServerObject::new();
    let ip_list = obj.get_server_ips("sgp");
    println!("ip_list: {:?}", ip_list);
    let server_list = obj.get_server_list();
    println!("server_list: {:?}", server_list);

    let ipt = iptables::new(false).unwrap();
}
