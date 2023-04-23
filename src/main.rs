use crossbeam::channel::{unbounded, Receiver, Sender};
use std::sync::Arc;
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

const MAX_ITER: u32 = 2000;
const BATCH_SIZE: usize = 75000;

type PixelJobBatch = Vec<(usize, usize, f64, f64)>;
type PixelResultBatch = Vec<(usize, usize, u32)>;

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

fn color(iter: u32, max_iter: u32) -> u32 {
    let t = iter as f64 / max_iter as f64;
    let b = (9.0 * (1.0 - t) * t * t * t * 255.0) as u32;
    let g = (15.0 * (1.0 - t) * (1.0 - t) * t * t * 255.0) as u32;
    let r = (8.5 * (1.0 - t) * (1.0 - t) * (1.0 - t) * t * 255.0) as u32;

    (0xff << 24) | (b << 16) | (g << 8) | r
}

fn worker_thread(job_rx: Arc<Receiver<PixelJobBatch>>, result_tx: Arc<Sender<PixelResultBatch>>) {
    while let Ok(job_batch) = job_rx.recv() {
        let mut result_batch = Vec::with_capacity(job_batch.len());

        for (x, y, cx, cy) in job_batch {
            let iter = mandelbrot(cx, cy, MAX_ITER);
            let pixel_color = color(iter, MAX_ITER);
            result_batch.push((x, y, pixel_color));
        }

        result_tx.send(result_batch).unwrap();
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
                send_jobs(size, &job_tx);
            }
            Event::RedrawRequested(_) => {
                if let Ok(result_batch) = result_rx.try_recv() {
                    let frame: &mut [u8] = pixels.frame_mut();
                    let width = window.inner_size().width as usize;
                    for (x, y, color) in result_batch {
                        let index = (y * width) + x;
                        let dst = &mut frame[4 * index..4 * (index + 1)];
                        dst.copy_from_slice(&color.to_ne_bytes());
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

    let mut job_batch = Vec::with_capacity(BATCH_SIZE);

    for y in 0..size.height {
        for x in 0..size.width {
            let cx = x as f64 * scale_x - 2.0;
            let cy = y as f64 * scale_y - 1.0;
            job_batch.push((x as usize, y as usize, cx, cy));

            if job_batch.len() == BATCH_SIZE {
                job_tx.send(job_batch).unwrap();
                job_batch = Vec::with_capacity(BATCH_SIZE);
            }
        }
    }

    if !job_batch.is_empty() {
        job_tx.send(job_batch).unwrap();
    }
}
