use ansi_term::Style;

use fs::fields as f;
use output::cell::TextCell;

impl f::Group {
    pub fn render<C: Colours, U>(&self, colours: &C, _: &U) -> TextCell {
        let style = colours.not_yours();

        // TODO: render appropriate group and owner
        TextCell::paint(style, self.0.to_string())
    }
}

pub trait Colours {
    fn yours(&self) -> Style;
    fn not_yours(&self) -> Style;
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
        fn yours(&self) -> Style {
            Fixed(80).normal()
        }
        fn not_yours(&self) -> Style {
            Fixed(81).normal()
        }
    }
}
