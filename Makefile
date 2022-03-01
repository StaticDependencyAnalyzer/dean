test:
	cargo test

test-entr:
	find . -name '*.rs' | entr -c $(MAKE) test

fmt:
	cargo fmt

fix:
	cargo fix --allow-dirty --allow-staged

docs:
	cargo doc
