.PHONY: rust-cpp-oracle

rust-cpp-oracle: version cataclysm.a
	$(MAKE) -C tests SOURCES="test_main.cpp fake_messages.cpp rust_cpp_oracle_item_pocket_test.cpp rust_cpp_oracle_item_group_test.cpp"
