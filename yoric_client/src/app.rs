use crate::canvas::YoricCanvas;
use crate::client::{ClientHandle, Topic};
// use crate::tweaks::YoricTweaks;

use cursive::theme::{Style, PaletteColor, Effect};
use cursive::utils::markup::StyledString;
use yoric_core::*;
use tokio::sync::oneshot;
use cursive::{Cursive, View, Printer};
use cursive::views::{Button, Dialog, DummyView, EditView,
                     LinearLayout, ListView, TextView, FocusTracker, Canvas, TextArea, Panel};
use cursive::traits::*;

struct SessionData {
    handle: ClientHandle,
    lobby_id: Option<String>,
    uname: Option<String>,
    state: Option<GameState>
}

impl SessionData {
    fn new(handle: ClientHandle) -> SessionData {
        SessionData {
            handle,
            lobby_id: None,
            uname: None,
            state: None
        }
    }
}

struct DataView {
    subscriptions: Vec<Subscription>,
    trackers: Vec<FocusTracker<Button>>
}

impl DataView {
    fn new(subscriptions: Vec<Subscription>) -> DataView {
        DataView { subscriptions, trackers: vec![] }
    }
}

impl View for DataView {
    fn draw(&self, _: &Printer) { }
}

struct Subscription {
    unsubscribe: Option<oneshot::Sender<()>>
}

impl Subscription {
    fn subscribe<F>(sink: cursive::CbSink,
                    handle: &ClientHandle,
                    topic: Topic,
                    cb: Box<F>) -> Subscription where
                    F: FnOnce(&mut Cursive, ServerMessage)
                        + std::marker::Send
                        + std::marker::Sync
                        + std::marker::Copy
                        + 'static {
        let mut handle = handle.clone();

        let (tx, mut rx) = oneshot::channel();

        tokio::spawn(async move {
            let mut sub_rec = handle.subscribe(topic);

            loop {
                let cb = cb.clone();
                tokio::select! {
                    _ = &mut rx => break,
                    Some(msg) = sub_rec.recv() => {
                        sink.send(Box::new(|s| cb(s, msg))).unwrap();
                    }
                }
            }
        });

        Subscription { unsubscribe: Some(tx) }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(unsub) = self.unsubscribe.take() {
            info!("Dropping subscription");
            unsub.send(()).unwrap();
        }
    }
}

pub fn start_app(handle: ClientHandle) {
    let mut siv = cursive::crossterm();

    siv.load_toml(include_str!("../theme.toml")).unwrap();

    siv.add_global_callback('q', |s| s.quit());

    let _sub = Subscription::subscribe(
        siv.cb_sink().clone(),
        &handle,
        Topic::Info,
        Box::new(|s: &mut Cursive, msg| {
            match msg {
                ServerMessage::LobbyID(id) => {
                    s.user_data::<SessionData>().unwrap().lobby_id = Some(id);
                    lobby(s);
                },
                _ => ()
            }
        }) // Explicit type bc the compiler hates type inference in closures
    );

    let data = SessionData::new(handle);
    siv.set_user_data(data);
    siv.add_layer(DummyView); // Dummy layer so every function can pop a layer

    get_uname(&mut siv);

    siv.run();
}

fn display_info(s: &mut Cursive, text: String) {
    s.add_layer(Dialog::around(TextView::new(text))
        .title("Info")
        .button("Ok", |s| { s.pop_layer(); }));
}

fn display_msg(s: &mut Cursive, msg: ServerMessage) {
    display_info(s, format!("Received: {:?}", msg));
}

fn get_uname(s: &mut Cursive) {
    fn set_uname(s: &mut Cursive, name: String) {
        let len = name.len();
        if len < 4 { display_info(s, String::from("Too short")); }
        else {
            s.user_data::<SessionData>().unwrap().uname = Some(name);
            select_lobby(s);
        }
    }

    s.pop_layer();
    s.add_layer(
        Dialog::new()
            .title("Choose a name")
            .padding_lrtb(1, 1, 1, 0)
            .content(
                EditView::new()
                    .max_content_width(16)
                    .on_submit(|s, name| {
                        set_uname(s, String::from(name));
                    })
                    .with_name("name")
                    .fixed_width(20),
            )
            .button("Ok", |s| {
                let name = s
                    .call_on_name("name", |view: &mut EditView| {
                        view.get_content()
                    })
                    .unwrap();
                    set_uname(s, (*name).clone());
            }),
    );
}

fn select_lobby(s: &mut Cursive) {
    let uname = s.user_data::<SessionData>().unwrap().uname.clone().unwrap();

    let buttons = LinearLayout::vertical()
        .child(Button::new_raw("[New Lobby]", new_lobby))
        .child(Button::new_raw("[Join Lobby]", join_lobby))
        .child(DummyView)
        .child(Button::new_raw("[Quit]", Cursive::quit));

    s.add_layer(Dialog::around(LinearLayout::horizontal()
        .child(DataView::new(vec![]).with_name("data"))
        .child(Dialog::text(format!("Welcome, {}", uname))
                   // .yoric_button("Change name", |s| { get_uname(s) })
               )
        .child(DummyView)
        .child(buttons))
        .title("Yoric"));
}

fn new_lobby(s: &mut Cursive) {
    let mut handle = s.user_data::<SessionData>().unwrap().handle.clone();
    let uname = s.user_data::<SessionData>().unwrap().uname.clone().unwrap();
    let button = Button::new_raw("[Back]", |s| { s.pop_layer(); });

    s.add_layer(Dialog::around(LinearLayout::vertical()
        .child(TextView::new("Requesting new lobby id..."))
        .child(button))
            .title("New Lobby"));

    handle.send(ClientMessage::Join(JoinRequest::NewLobby, uname));
}

fn join_lobby(s: &mut Cursive) {
    s.add_layer(
        Dialog::new()
            .title("Join Lobby")
            .padding_lrtb(1, 1, 1, 0)
            .content(
                EditView::new()
                    .on_submit(|s, id| {
                        let mut handle = s.user_data::<SessionData>().unwrap().handle.clone();
                        let uname = s.user_data::<SessionData>().unwrap().uname.clone().unwrap();
                        handle.send(ClientMessage::Join(JoinRequest::JoinLobby(String::from(id)), uname));
                    })
                    .with_name("lobby_id")
                    .fixed_width(20),
            )
            .button("Back", |s| { s.pop_layer(); })
            .button("Ok", |s| {
                let id = s
                    .call_on_name("lobby_id", |view: &mut EditView| {
                        view.get_content()
                    })
                    .unwrap();

                    let mut handle = s.user_data::<SessionData>().unwrap().handle.clone();
                    let uname = s.user_data::<SessionData>().unwrap().uname.clone().unwrap();
                    handle.send(ClientMessage::Join(JoinRequest::JoinLobby((*id).clone()), uname));
            }),
        );
}

fn lobby(s: &mut Cursive) {
    let handle: ClientHandle;
    let lobby_id: Option<String>;
    {
        let data = s.user_data::<SessionData>().unwrap();
        handle = data.handle.clone();
        lobby_id = data.lobby_id.clone();
    }

    let start_handle = handle.clone();

    let start_button = Button::new_raw("[Start]", move |_| { start_handle.clone().send(ClientMessage::Start); })
        .disabled()
        .with_name("start_button");
    let buttons = LinearLayout::horizontal()
        .child(Button::new_raw("[Back]", |s| {
            s.user_data::<SessionData>().unwrap().handle.send(ClientMessage::Leave);
            s.pop_layer();
        }))
        .child(start_button);

    let sub = Subscription::subscribe(
        s.cb_sink().clone(),
        &handle,
        Topic::Info,
        Box::new(|s: &mut Cursive, msg| {
            let text_host = StyledString::styled("(host)", Style::from(PaletteColor::Tertiary).combine(Effect::Bold));

            match msg {
                ServerMessage::Lobby(LobbyState { count, unames, host }) => {
                    s.call_on_name("player_list", |view: &mut ListView| {
                        view.clear();

                        info!("{} players", &count);
                        info!("names, {:?}", &unames);
                        view.add_child(&host, TextView::new(text_host));
                        for name in unames {
                            view.add_child(&name, TextView::new(" "));
                        }


                    }).unwrap();

                    let my_name = s.user_data::<SessionData>().unwrap().uname.clone().unwrap();
                    if host.eq(&my_name) && count >= 1 { // Change to 3 eventually
                        s.call_on_name("start_button", |button: &mut Button| {
                            button.enable();
                        });
                    }
                },
                ServerMessage::Starting => game(s),
                _ => ()
            }
        })
    );

    if let Some(id) = lobby_id {
        s.pop_layer();
        s.add_layer(Dialog::around(LinearLayout::vertical()
            .child(
                ListView::new()
                    .child("", DataView::new(vec![sub]))
                    .child("", ListView::new()
                            .with_name("player_list")
                            .scrollable()
                    )
                )
            .child(buttons))
            .title(format!("Lobby {}", &id))
            .full_screen()
        );
    }

}

fn game(s: &mut Cursive) {
    let lobby_id: String;
    let handle: ClientHandle;
    let uname: String;
    {
        let data = s.user_data::<SessionData>().unwrap();
        handle = data.handle.clone();
        lobby_id = data.lobby_id.clone().unwrap();
        uname = data.uname.clone().unwrap();
    }

    let sub_state = Subscription::subscribe(
        s.cb_sink().clone(),
        &handle,
        Topic::State,
        Box::new(|s: &mut Cursive, msg: ServerMessage| {
            s.call_on_name("canvas", |canvas: &mut YoricCanvas| {
                match msg {
                    ServerMessage::PartialState(msg) => canvas.update(msg),
                    _ => ()
                }
            });
        })
    );

    let sub_chat = Subscription::subscribe(
        s.cb_sink().clone(),
        &handle,
        Topic::Chat,
        Box::new(|s: &mut Cursive, msg: ServerMessage| {
            match msg {
                ServerMessage::Chat(_, _) => display_msg(s, msg),
                _ => ()
            }
            
        })
    );

    s.pop_layer();
    s.add_layer(Dialog::new()
        .content(LinearLayout::vertical()
            .child(DataView::new(vec![sub_state, sub_chat]))
            .child(YoricCanvas::new(GameState::new(), uname.clone())
                   .with_name("canvas")
                   .full_height())
            .child(LinearLayout::horizontal()
                   .child(Panel::new(ListView::new()
                                     .child("Play Skull", Button::new("Play Skull", |s| {
                                         let mut handle = s.user_data::<SessionData>().unwrap().handle.clone();
                                         handle.send(ClientMessage::Do(Command::Play(Card::Skull)))
                                     }))
                                     .child("Play Rose", Button::new("Play Rose", |s| {
                                         let mut handle = s.user_data::<SessionData>().unwrap().handle.clone();
                                         handle.send(ClientMessage::Do(Command::Play(Card::Rose)))
                                     }))
                                     .child("Bid", EditView::new()
                                         .max_content_width(2)
                                         .on_submit(|s, bid| {
                                              if let Ok(bid) = bid.parse() { 
                                                  let mut handle = s.user_data::<SessionData>().unwrap().handle.clone();
                                                  handle.send(ClientMessage::Do(Command::Bid(bid)));
                                              }
                                         })
                                         .fixed_width(2),
                                     )
                                     .child("Pass", Button::new("Pass", |s| {
                                         let mut handle = s.user_data::<SessionData>().unwrap().handle.clone();
                                         handle.send(ClientMessage::Do(Command::Pass))
                                     }))
                                     .scrollable())
                          .title("Actions")
                          .full_width())
                   .child(Panel::new(TextArea::new()
                                     .scrollable())
                          .title("Chat")
                          .full_width())
                   .fixed_height(10)
            )
        )
        .title(format!("Game {}", lobby_id))
        .full_screen()
    );
}
