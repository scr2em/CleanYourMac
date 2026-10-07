# Verification

The Swift prototype passed 27 tests on macOS 26.5.1 with Swift 6.3.3 before the Rust migration began. Native fixture checks covered a disposable Trash round trip, generated orphan processes, and a newly created simulator. Existing user resources were not used for mutation tests.

The packaged Swift app was built for Apple silicon and Intel, ad hoc signed, and inspected in an isolated demo bundle. Demo mode disables cleanup actions.

The Rust migration is in progress. Rust-specific test and package results will be recorded here when its integration is complete. Figma publication currently requires connector reauthentication.

Run the repeatable checks with:

~~~sh
python3 scripts/lint-design.py
bash scripts/swift.sh test
bash scripts/test-native.sh
~~~

Native tests are opt-in and require a usable iOS simulator runtime. They create and clean up their own resources.
