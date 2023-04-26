use crossbeam::channel::{unbounded, Receiver, Sender};
use std::sync::Arc;
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

const MAX_ITER: u32 = 2000;
const BATCH_SIZE: u32 = 20000;

struct PixelJobBatch {
    pub start_x: u32,
    pub start_y: u32,
    pub window_width: u32,
    pub window_height: u32,
    pub scale_x: f64,
    pub scale_y: f64,
    pub count: u32,
}
struct PixelResultBatch {
    pub start_x: u32,
    pub start_y: u32,
    pub window_width: u32,
    pub window_height: u32,
    pub pixels: Vec<u8>, //heap to avoid copy of bulk data
    //pub count: u32,
}

fn mandelbrot(cx: f64, cy: f64, max_iter: u32) -> u32 {
    let mut x = 0.0;
    let mut y = 0.0;
    let mut iter = 0;

    while x * x + y * y <= 4.0 && iter < max_iter {
        let x_temp = x * x - y * y + cx;
        y = 2.0 * x * y + cy;
        x = x_temp;
        iter += 1;
    }

    iter
}

fn color(iter: u32, max_iter: u32) -> [u8;4] {
    let t = iter as f64 / max_iter as f64;
    let b = (9.0 * (1.0 - t) * t * t * t * 255.0) as u8;
    let g = (15.0 * (1.0 - t) * (1.0 - t) * t * t * 255.0) as u8;
    let r = (8.5 * (1.0 - t) * (1.0 - t) * (1.0 - t) * t * 255.0) as u8;

    [0xff, b, g, r]
}

fn worker_thread(job_rx: Arc<Receiver<PixelJobBatch>>, result_tx: Arc<Sender<PixelResultBatch>>) {
    while let Ok(job_batch) = job_rx.recv() {
        let mut pixels = Vec::with_capacity((job_batch.count * 4) as usize);
        for count in 0..job_batch.count {
            let x = job_batch.start_x + (count % job_batch.window_width);
            let y = job_batch.start_y + (count / job_batch.window_width);
            let cx = (x as f64 * job_batch.scale_x) - 2.0;
            let cy = (y as f64 * job_batch.scale_y) - 1.0;
            let iterations = mandelbrot(cx, cy, MAX_ITER);
            let pixel_color = color(iterations, MAX_ITER);
            pixels.extend_from_slice(&pixel_color);
        }
        result_tx.send(PixelResultBatch {
            start_x: job_batch.start_x,
            start_y: job_batch.start_y,
            window_width: job_batch.window_width,
            window_height: job_batch.window_height,
            pixels: pixels,
        }).unwrap();
    }
}

fn spawn_workers(
    num_workers: usize,
    job_rx: Arc<Receiver<PixelJobBatch>>,
    result_tx: Arc<Sender<PixelResultBatch>>,
) {
    for _ in 0..num_workers {
        let job_rx = job_rx.clone();
        let result_tx = result_tx.clone();
        std::thread::spawn(move || worker_thread(job_rx, result_tx));
    }
}

fn main() {
    let event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Mandelbrot Fractal")
        .with_inner_size(LogicalSize::new(2000.0, 1000.0))
        .build(&event_loop)
        .unwrap();

    let (job_tx, job_rx) = unbounded::<PixelJobBatch>();
    let (result_tx, result_rx) = unbounded::<PixelResultBatch>();
    let job_rx = Arc::new(job_rx);
    let result_tx = Arc::new(result_tx);

    let num_workers = num_cpus::get();
    spawn_workers(num_workers, job_rx.clone(), result_tx.clone());

    // Create the surface texture outside of the event loop
    let surface_texture = pixels::SurfaceTexture::new(
        window.inner_size().width as u32,
        window.inner_size().height as u32,
        &window,
    );

    // Create the Pixels instance outside of the event loop
    let mut pixels = pixels::Pixels::new(
        window.inner_size().width as u32,
        window.inner_size().height as u32,
        surface_texture,
    )
    .unwrap();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(std::time::Instant::now() + std::time::Duration::from_millis(16));
        //*control_flow = ControlFlow::Poll;

        // Call send_jobs() at the beginning of the event loop
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| {
            send_jobs(window.inner_size(), &job_tx);
        });

        match event {
            Event::RedrawEventsCleared => {
                window.request_redraw();
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                window_id,
            } if window_id == window.id() => {
                let surface_texture = pixels::SurfaceTexture::new(
                    size.width as u32,
                    size.height as u32,
                    &window,
                );            
                // Create the Pixels instance outside of the event loop
                pixels = pixels::Pixels::new(
                    size.width as u32,
                    size.height as u32,
                    surface_texture,
                )
                .unwrap();
                //pixels.frame_mut().fill(0);
                send_jobs(size, &job_tx);
            }
            Event::RedrawRequested(_) => {
                while let Ok(result_batch) = result_rx.try_recv() {
                    if result_batch.window_width  == window.inner_size().width  && 
                       result_batch.window_height == window.inner_size().height {
                    let frame: &mut [u8] = pixels.frame_mut();
                    let starting_offset = ((result_batch.start_y * result_batch.window_width * 4) + result_batch.start_x * 4)  as usize;
                    let ending_offset = starting_offset + (result_batch.pixels.len()) as usize;
                    frame[starting_offset .. ending_offset]
                        .copy_from_slice(&result_batch.pixels);
                    }
                }                
                pixels.render().unwrap();
            }

            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                window_id,
            } if window_id == window.id() => {
                *control_flow = ControlFlow::Exit;
            }
            _ => (),
        }
    });
}

fn send_jobs(size: winit::dpi::PhysicalSize<u32>, job_tx: &Sender<PixelJobBatch>) {
    let scale_x = 3.0 / size.width as f64;
    let scale_y = 2.0 / size.height as f64;

    let batches = ((size.width * size.height) / BATCH_SIZE as u32) + 1;
    
    for batch in 0..batches {
        let start_x = (batch * BATCH_SIZE as u32) % size.width;
        let start_y = (batch * BATCH_SIZE as u32) / size.width;
        let window_width = size.width;
        let window_height = size.height;
        
        let scale_x = scale_x;
        let scale_y = scale_y;
        let count = if batch == batches-1 {(size.width * size.height) % BATCH_SIZE as u32}
                         else {BATCH_SIZE};
        job_tx.send(PixelJobBatch {
            start_x,
            start_y,
            window_width,
            window_height,
            count,
            scale_x,
            scale_y,
        }).unwrap();
    }
}
