# rusty almonds

This project is a simple Rust application, rendering the Mandelbrot fractal. Its a learning sample for me. It uses work distribution via crossbeam transceivers and winit/pixels rendering into a window. Workload distribution is total overkill for the mandelbrot set without any zoom, but becomes essential for higher zoom levels. Interesting.