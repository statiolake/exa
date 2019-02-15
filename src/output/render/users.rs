use ansi_term::Style;

use fs::fields as f;
use output::cell::TextCell;

impl f::User {
    pub fn render<C: Colours, U>(&self, colours: &C, _: &U) -> TextCell {
        // TODO: render appropriate username and style
        let user_name = self.0.to_string();
        let style = colours.you();

        TextCell::paint(style, user_name)
    }
}

pub trait Colours {
    fn you(&self) -> Style;
    fn someone_else(&self) -> Style;
}

#[cfg(test)]
#[allow(unused_results)]
pub mod test {
    use super::Colours;

    use ansi_term::Colour::*;
    use ansi_term::Style;

    #[allow(dead_code)]
    struct TestColours;

    impl Colours for TestColours {
        fn you(&self) -> Style {
            Red.bold()
        }
        fn someone_else(&self) -> Style {
            Blue.underline()
        }
    }
}
