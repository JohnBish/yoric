use yoric_core::{GameState, Card};
use cursive::{view::View, Printer, XY, Vec2};

pub struct YoricCanvas {
    state: GameState,
    uname: String
}

impl YoricCanvas {
    pub fn new(state: GameState, uname: String) -> YoricCanvas {
        YoricCanvas { state, uname }
    }

    pub fn update(&mut self, state: GameState) {
        self.state = state;
    }

    fn draw_card_stack(&self, printer: &Printer, top_card: &Card, count: usize, uname: &String) {
        if count == 0 { return }

        for i in (1..count).rev() {
            printer.print_box((i, i), (14, 10), false);
        }

        self.draw_card(printer, top_card, uname);
    }

    fn draw_card(&self, printer: &Printer, card: &Card, uname: &String) {
        static WIDTH: usize = 14;
        let name_offset = (WIDTH - self.uname.len()) / 2;

        printer.print_box((0, 0), (14, 10), false);

        match card {
            Card::Skull => YoricCanvas::draw_skull(printer),
            Card::Rose => YoricCanvas::draw_rose(printer),
            Card::Unknown => YoricCanvas::draw_unknown(printer)
        }

        printer.print((name_offset, 8), &uname);
    }

    fn draw_skull(printer: &Printer) {
        printer.print((1, 1), "            " );
        printer.print((1, 2), "  ,'   Y`.  " );
        printer.print((1, 3), " /        \\ ");
        printer.print((1, 4), " \\ ()  () /" );
        printer.print((1, 5), "  `. /\\ ,' " );
        printer.print((1, 6), "   `LLLU'   " );
        printer.print((1, 7), "            " );
        printer.print((1, 8), "            " );
    }

    fn draw_rose(printer: &Printer) {
        printer.print((1, 1), "            " );
        printer.print((1, 2), "   ,  _, ,  " );
        printer.print((1, 3), "  /(/  \\)\\ ");
        printer.print((1, 4), "  '.\\__/.' " );
        printer.print((1, 5), " .-'`/\\`'-." );
        printer.print((1, 6), " \\_/|  |\\_/");
        printer.print((1, 7), "     \\/    " );
        printer.print((1, 8), "            " );
    }

    fn draw_unknown(printer: &Printer) {
        printer.print((1, 1), "░░░░░░░░░░░░");
        printer.print((1, 2), "░░░░░░░░░░░░");
        printer.print((1, 3), "░░░░░░░░░░░░");
        printer.print((1, 4), "░░░░░░░░░░░░");
        printer.print((1, 5), "░░░░░░░░░░░░");
        printer.print((1, 6), "░░░░░░░░░░░░");
        printer.print((1, 7), "░░░░░░░░░░░░");
        printer.print((1, 8), "░░░░░░░░░░░░");
    }
}


impl View for YoricCanvas {
    fn draw(&self, printer: &Printer) {
        let width = printer.size.x;
        let height = printer.size.y;

        if let Some(player_self) = self.state.players.get(&self.uname) {
            let cards_in_hand = &player_self.cards_in_hand;

            if cards_in_hand.contains(&Card::Skull) {
                self.draw_card(&printer.offset((0, height - 16)), &Card::Skull, &self.uname);
            }
            for i in 1..=cards_in_hand.roses {
                self.draw_card(&printer.offset((i as usize * 15, height - 16)), &Card::Rose, &self.uname);
            }
        }

        for (idx, (uname, player)) in self.state.players.iter().enumerate() {
            if !&player.cards_in_play.is_empty() {
                self.draw_card_stack(&printer.offset((idx*15, 0)), &Card::Unknown, player.cards_in_play.len(), uname);
            }
        }
    }
}
