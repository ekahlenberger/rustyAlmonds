use crossbeam::channel::{unbounded, Receiver, Sender};
use std::sync::Arc;
use winit::{
    dpi::LogicalSize,
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

const MAX_ITER: u32 = 100;
const BATCH_SIZE: u32 = 12000;

struct PixelJobBatch {
    pub start_x: u32,
    pub start_y: u32,
    pub window_width: u32,
    pub window_height: u32,
    pub scale_x: f64,
    pub scale_y: f64,
    pub count: u32,
    pub offset_x: f64,
    pub offset_y: f64,
    pub zoom: f64,
}
struct PixelResultBatch {
    pub start_x: u32,
    pub start_y: u32,
    pub window_width: u32,
    pub window_height: u32,
    pub pixels: Vec<u8>, //heap to avoid copy of bulk data
}

struct RenderOffset {
    zoom: f64,
    offset_x: f64,
    offset_y: f64,
}

struct MousePosition {
    x: f64,
    y: f64,
    left_button_pressed: bool,
    drag_start_x: f64,
    drag_start_y: f64,
}

fn mandelbrot(cx: f64, cy: f64, max_iter: u32) -> u32 {
    let mut x = 0.0;
    let mut y = 0.0;
    let mut iter: u32 = 0;
    let mut x2 = 0.0;
    let mut y2 = 0.0;

    // Initialize a buffer for periodicity checking
    let check_interval = 20;
    let mut buffer: Vec<(f64, f64)> = Vec::with_capacity(check_interval);

    while x2 + y2 <= 4.0 && iter < max_iter {
        y = 2.0 * x * y + cy;
        x = x2 - y2 + cx;
        x2 = x * x;
        y2 = y * y;    

        // Periodicity checking
        if iter % check_interval as u32 == 0 {
            // Check for any repeating patterns in the buffer
            if buffer.iter().any(|&(prev_x, prev_y)| prev_x == x && prev_y == y) {
                return max_iter; // Break the loop when a cycle is detected
            }
            // Add the current point to the buffer
            buffer.push((x, y));
            // Remove the oldest point from the buffer if it's full
            if buffer.len() > check_interval {
                buffer.remove(0);
            }
        }

        iter += 1;
    }

    iter
}

fn color(iter: u32, max_iter: u32) -> [u8; 4] {
    let t = iter as f64 / max_iter as f64;
    let b = (9.0 * (1.0 - t) * t * t * t * 255.0) as u8;
    let g = (15.0 * (1.0 - t) * (1.0 - t) * t * t * 255.0) as u8;
    let r = (8.5 * (1.0 - t) * (1.0 - t) * (1.0 - t) * t * 255.0) as u8;

    [0xff, b, g, r]
}

fn worker_thread(job_rx: Arc<Receiver<PixelJobBatch>>, result_tx: Arc<Sender<PixelResultBatch>>) {
    while let Ok(job_batch) = job_rx.recv() {
        let mut pixels = Vec::with_capacity((job_batch.count * 4) as usize);
        let mut x = job_batch.start_x;
        let mut y = job_batch.start_y;
        let max_iter = (MAX_ITER as f64 / job_batch.zoom * 1.1).ceil() as u32;
        for _ in 0..job_batch.count {
            x += 1;
            if x >= job_batch.window_width {
                x = 0;
                y += 1;
            }
            let cx = (x as f64 * job_batch.scale_x) - 2.0 + job_batch.offset_x;
            let cy = (y as f64 * job_batch.scale_y) - 1.0 + job_batch.offset_y;
            let iterations = mandelbrot(cx, cy, max_iter);
            let pixel_color = color(iterations, max_iter);
            pixels.extend_from_slice(&pixel_color);
        }
        result_tx
            .send(PixelResultBatch {
                start_x: job_batch.start_x,
                start_y: job_batch.start_y,
                window_width: job_batch.window_width,
                window_height: job_batch.window_height,
                pixels: pixels,
            })
            .unwrap();
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
        .with_inner_size(LogicalSize::new(2345, 1234))
        .build(&event_loop)
        .unwrap();

    let mut render_offset = RenderOffset {zoom: 1.0,offset_x: 0.0,offset_y: 0.0,};
    let mut mouse = MousePosition { x: 0.0, y: 0.0, left_button_pressed: false, drag_start_x: 0.0, drag_start_y: 0.0, };

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

    let mut last_zoom_change: Option<std::time::Instant> = None;

    event_loop.run(move |event, _, control_flow| {
        *control_flow =
            ControlFlow::WaitUntil(std::time::Instant::now() + std::time::Duration::from_millis(8));
            //*control_flow = ControlFlow::Poll;      

        match event {
            Event::RedrawEventsCleared => {
                window.request_redraw();
            }
            Event::WindowEvent {
                event: WindowEvent::Resized(size),
                window_id,
            } if window_id == window.id() => {
                // Update the surface texture and pixels instance
                let surface_texture = pixels::SurfaceTexture::new(size.width, size.height, &window);
                pixels = pixels::Pixels::new(size.width, size.height, surface_texture).unwrap();

                // Send new jobs with the updated window size
                send_jobs(size.into(), &job_tx, &render_offset);
            }
            Event::RedrawRequested(_) => {
                let frame: &mut [u8] = pixels.frame_mut();
                let size = window.inner_size();
                while let Ok(result_batch) = result_rx.try_recv() {
                    if result_batch.window_width == size.width &&
                       result_batch.window_height == size.height
                    {
                        let starting_offset =((result_batch.start_y * result_batch.window_width * 4)+ result_batch.start_x * 4) as usize;
                        let ending_offset = starting_offset + (result_batch.pixels.len()) as usize;
                        frame[starting_offset..ending_offset].copy_from_slice(&result_batch.pixels);
                    }
                }

                if let Some(instant) = last_zoom_change {
                    if instant.elapsed().as_millis() > 200 {
                        // some time has passed since the last zoom change, all mouse wheel events have been processed
                        while let Ok(_) = job_rx.try_recv() {} // swallow all jobs in the queue, as they are outdated
                        send_jobs(size.into(), &job_tx, &render_offset);
                        last_zoom_change = None;
                    }
                }
                
                pixels.render().unwrap();
            }
            Event::WindowEvent { window_id, event: WindowEvent::MouseWheel { delta,..} 
            } if window_id == window.id() => {
                const ZOOM_FACTOR: f64 = 1.05;
                match delta {
                    winit::event::MouseScrollDelta::LineDelta(_, y) => {
                        let old_zoom = render_offset.zoom;
                        if y > 0.0 {
                            render_offset.zoom /= ZOOM_FACTOR;
                        } else if y < 0.0 {
                            render_offset.zoom *= ZOOM_FACTOR;
                        }
                        let size = window.inner_size();
            
                        let new_scale_x = 3.0 / size.width as f64 * render_offset.zoom;
                        let new_scale_y = 2.0 / size.height as f64 * render_offset.zoom;
                        let scale_x = 3.0 / size.width as f64 * old_zoom;
                        let scale_y = 2.0 / size.height as f64 * old_zoom;

                        render_offset.offset_x += mouse.x * (scale_x - new_scale_x);
                        render_offset.offset_y += mouse.y * (scale_y - new_scale_y);

                        last_zoom_change = std::time::Instant::now().into();
                    }
                    _ => (),
                }
            }
            Event::WindowEvent {
                event: WindowEvent::MouseInput { state, button, .. },
                window_id,
            } if window_id == window.id() => {
                match button {
                    winit::event::MouseButton::Left => {
                        mouse.left_button_pressed = state == winit::event::ElementState::Pressed;
                        if !mouse.left_button_pressed {
                            let size = window.inner_size();
                            let scale_x = 3.0 / size.width as f64 * render_offset.zoom;
                            let scale_y = 2.0 / size.height as f64 * render_offset.zoom;
                            // Update render_offset when the left mouse button is released
                            render_offset.offset_x -= (mouse.x - mouse.drag_start_x) * scale_x;
                            render_offset.offset_y -= (mouse.y - mouse.drag_start_y) * scale_y;

                            while let Ok(_) = job_rx.try_recv() {} // swallow all jobs in the queue, as they are outdated
                            send_jobs(window.inner_size(), &job_tx, &render_offset);
                        } else {
                            // Store the initial mouse position when the left mouse button is pressed
                            mouse.drag_start_x = mouse.x;
                            mouse.drag_start_y = mouse.y;
                        }
                    }
                    _ => (),
                }
            }            
            Event::WindowEvent {
                event: WindowEvent::CursorMoved { position, .. },
                window_id,
            } if window_id == window.id() => {
                mouse.x = position.x;
                mouse.y = position.y;
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

fn send_jobs(size: winit::dpi::PhysicalSize<u32>, job_tx: &Sender<PixelJobBatch>, render_offset: &RenderOffset) {
    let scale_x = 3.0 / size.width as f64 * render_offset.zoom;
    let scale_y = 2.0 / size.height as f64 * render_offset.zoom;

    let pixel_count = size.width * size.height;
    let batches = (pixel_count / BATCH_SIZE as u32) + 1;

    for batch in 0..batches {
        let start_x = (batch * BATCH_SIZE as u32) % size.width;
        let start_y = (batch * BATCH_SIZE as u32) / size.width;

        let count = if batch == batches - 1 {
            pixel_count % BATCH_SIZE as u32
        } else {
            BATCH_SIZE
        };
        job_tx
            .send(PixelJobBatch {
                start_x,
                start_y,
                window_width: size.width,
                window_height: size.height,
                count,
                scale_x,
                scale_y,
                offset_x: render_offset.offset_x,
                offset_y: render_offset.offset_y,
                zoom: render_offset.zoom,
            })
            .unwrap();
    }
}
