test:
	cargo test --all-features

test-entr:
	find . -name '*.rs' | entr -c $(MAKE) test

lint:
	cargo clippy

fmt:
	cargo +nightly fmt

fix:
	cargo fix --allow-dirty --allow-staged
	cargo clippy --fix --allow-dirty --allow-staged

docs:
	cargo doc
