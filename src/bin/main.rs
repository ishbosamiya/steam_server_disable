use iced::Sandbox;
use steam_server_disable::ui::UI;
// use steam_server_disable::ServerObject;

fn main() -> iced::Result {
    // let obj = ServerObject::new();
    // let server_list = obj.get_server_list();

    // for server in &server_list {
    //     let ip_list = obj.get_server_ips(server);
    //     println!("{}: {:?}", server, ip_list);
    // }

    // let _ipt = iptables::new(false).unwrap();

    UI::run(iced::Settings::default())
}
