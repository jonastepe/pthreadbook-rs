# pthreadbook-rs
The purpose is to reimplement the examples from the book [Programming with POSIX Threads](http://www.informit.com/store/programming-with-posix-threads-9780201633924) with Rust by using the Rust standard library. Only in situations where the desired functionality is not covered by the standard library will the code use libc to achieve its goal.

## Why?
I like Rust and C (especially in the context of Linux system programming) and want to learn more about multi-threaded programming in Rust and C. That is a nice way to combine all those goals.

Also, I am always looking for feedback from people who are more experienced than I. So, if you want to give me some advice on how to make the examples better or more idiomatic Rust, please do.

## How to run the examples?
The project contains a binary for every example program. To run each one individually:

```
cargo run --release --bin <name-of-bin-from-Cargo.toml>
```

The `--release` is necessary to achieve behaviour similar to the C versions of the examples.

