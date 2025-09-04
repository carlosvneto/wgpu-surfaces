#[path = "../common/app.rs"]
mod app;
#[path = "../common/vertex.rs"]
mod vertex;
mod state;

use winit::event_loop::EventLoop;

use crate::app::Application;

fn main() {
    let mut sample_count = 1 as u32;
    let mut colormap_name = "jet";
    let mut wireframe_color = "white";
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        sample_count = args[1].parse::<u32>().unwrap();
    }
    if args.len() > 2 {
        colormap_name = &args[2];
    }
    if args.len() > 3 {
        wireframe_color = &args[3];
    }

    let title = "ch02 simple surface";

    let _ = run(sample_count, colormap_name, wireframe_color, title);

    pub fn run(
        sample_count: u32,
        colormap_name: &str,
        wireframe_color: &str,
        title: &str,
    ) -> anyhow::Result<()> {
        env_logger::init();

        let event_loop = EventLoop::builder().build()?;
        let mut app = Application::new(sample_count, colormap_name, wireframe_color, title, None);

        event_loop.run_app(&mut app)?;

        Ok(())
    }
}
