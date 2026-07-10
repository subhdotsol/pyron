pub mod badge;
pub mod html;
pub mod terminal;

pub use terminal::{
    clear_progress, print_cpi_tree, print_footer, print_header, print_instruction_stats,
    print_simulating,
};
