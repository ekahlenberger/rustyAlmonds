# Rusty Almonds

This project is a learning experience for me in understanding workload parallelization and window management using the Rust programming language. The main focus of this project is to leverage the power of Rust to create an efficient and highly performant Mandelbrot fractal renderer.

## Libraries used

- [__crossbeam__](https://lib.rs/crates/crossbeam): Provides the crossbeam-channel crate for fast and easy-to-use multi-producer multi-consumer channels.
- [__winit__](https://lib.rs/crates/winit): A platform-agnostic window handling library to create windows and handle input events.
- [__pixels__](https://lib.rs/crates/pixels): A tiny hardware-accelerated pixel frame buffer for building 2D applications. Initially, I could not figure out how to use winit to draw to my window. Well, winit does not provide that functionality. The solution was to combine winit and pixels, which was a valuable lesson learned. Great library.

## Performance Considerations

When optimizing the performance of a program, it's essential to understand both the problem domain and the programming language, as well as the models being used. This project leverages Rust's powerful concurrency model to achieve efficient parallelization of the rendering workload.

### Problem domain optimizations

- Early escapes for the main cardioid and period 2 bulb
- Early escapes for periodic calculations
- Optimizing the mandelbrot calculation away from the first naive approach
- Gradient checking, escaping if changes are so small that escaping the Mandelbrot set seems implausible.

### Language domain optimizations

Another optimization using Rust is to avoid copying results of thousands of pixels back into the main render thread. Instead, a heap-allocated __Vector__ is used, and only a reference to the data is copied. This approach also helps avoid stack size problems. __Crossbeam__ makes inter-thread communication really easy. Furthermore, in one of the first iterations, all the coordinates of pixels that had to be calculated were copied to the worker threads. Now, only the start value and the needed amount are copied.

__Think about your problems, test your ideas.__

## Getting Started

Clone the repository and

    cargo run --release # compiling takes time as lto is activated

