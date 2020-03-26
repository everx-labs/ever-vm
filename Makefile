all: Makefile 
	@cargo test --release --features "ci_run" -- --nocapture --test-threads=1 


