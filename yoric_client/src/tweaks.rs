use std::ops::{Deref, DerefMut};

use cursive::{View, views::{Dialog, Button}, Cursive, With, event::{EventResult, Event}, view::{ViewWrapper, CannotFocus}, direction::Direction, wrap_impl};

pub struct YoricButton {
    button: Button,
    on_focus_lost: Box<dyn FnMut(&mut Button) -> EventResult>,
    on_focus: Box<dyn FnMut(&mut Button) -> EventResult>,
}

impl YoricButton {
    pub fn new(b: Button) -> YoricButton {
        YoricButton {
            button: b,
            on_focus_lost: Box::new(|_| EventResult::Ignored),
            on_focus: Box::new(|_| EventResult::Ignored)
        }
    }

    #[must_use]
    pub fn on_focus<F>(self, f: F) -> Self
    where
        F: 'static + FnMut(&mut Button) -> EventResult,
    {
        self.with(|s| s.on_focus = Box::new(f))
    }

    #[must_use]
    pub fn on_focus_lost<F>(self, f: F) -> Self
    where
        F: 'static + FnMut(&mut Button) -> EventResult,
    {
        self.with(|s| s.on_focus_lost = Box::new(f))
    }
}

impl ViewWrapper for YoricButton {
    wrap_impl!(self.button: Button);

    fn wrap_take_focus(
        &mut self,
        source: Direction,
    ) -> Result<EventResult, CannotFocus> {
        match self.button.take_focus(source) {
            Ok(res) => Ok(res.and((self.on_focus)(&mut self.button))),
            Err(CannotFocus) => Err(CannotFocus),
        }
    }

    fn wrap_on_event(&mut self, event: Event) -> EventResult {
        let res = if let Event::FocusLost = event {
            (self.on_focus_lost)(&mut self.button)
        } else {
            EventResult::Ignored
        };
        res.and(self.button.on_event(event))
    }
}

impl Deref for YoricButton {
    type Target = Button;

    fn deref(&self) -> &Self::Target {
        &self.button
    }
}

impl DerefMut for YoricButton {
    fn deref_mut(&mut self) -> &mut Self::Target {
       &mut self.button
    }
}

pub trait YoricTweaks {
    fn yoric_button<F, S>(self, label: S, cb: F) -> Dialog where
        S: Into<String>,
        F: 'static + Fn(&mut Cursive);
}

impl YoricTweaks for Dialog {
    #[must_use]
    fn yoric_button<F, S>(mut self, label: S, cb: F) -> Dialog where
        S: Into<String>,
        F: 'static + Fn(&mut Cursive),
    {
        let label = label.into();
        self.add_button(label.clone(), cb);
        self.with(|s| {
            s.buttons_mut()
                .last()
                .unwrap() = YoricButton::new(Button::new("", |_| {}));


            // button.set_label_raw(format!(" [{}] ", &label);        })

    }
}
