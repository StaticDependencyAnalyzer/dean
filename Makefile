test:
	cargo test

test-entr:
	find . -name '*.rs' | entr -c $(MAKE) test

fmt:
	cargo fmt

fix:
	cargo fix

docs:
	cargo doc
