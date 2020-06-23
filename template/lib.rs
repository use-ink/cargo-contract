#![cfg_attr(not(feature = "std"), no_std)]

use ink_lang as ink;

#[ink::contract(version = "0.1.0")]
mod {{name}} {
    use ink_core::storage;

    /// Defines the storage of your contract.
    /// Add new fields to the below struct in order
    /// to add new static storage fields to your contract.
    #[ink(storage)]
    struct {{camel_name}} {
        /// Stores a single `bool` value on the storage.
        value: bool,
    }

    impl {{camel_name}} {
        /// Constructor that initializes the `bool` value to the given `init_value`.
        #[ink(constructor)]
        fn new(init_value: bool) -> Self {
            self.value.set(init_value);
        }

        /// Constructor that initializes the `bool` value to `false`.
        ///
        /// Constructors can delegate to other constructors.
        #[ink(constructor)]
        fn default() -> Self {
            Self::new(Default::default())
        }

        /// A message that can be called on instantiated contracts.
        /// This one flips the value of the stored `bool` from `true`
        /// to `false` and vice versa.
        #[ink(message)]
        fn flip(&mut self) {
            self.value = !self.value;
        }

        /// Simply returns the current value of our `bool`.
        #[ink(message)]
        fn get(&self) -> bool {
            self.value
        }
    }

    /// Unit tests in Rust are normally defined within such a `#[cfg(test)]`
    /// module and test functions are marked with a `#[test]` attribute.
    /// The below code is technically just normal Rust code.
    #[cfg(test)]
    mod tests {
        /// Imports all the definitions from the outer scope so we can use them here.
        use super::*;

        /// We test if the default constructor does its job.
        #[test]
        fn default_works() {
            let {{name}} = {{camel_name}}::default();
            assert_eq!({{name}}.get(), false);
        }

        /// We test a simple use case of our contract.
        #[test]
        fn it_works() {
            let mut {{name}} = {{camel_name}}::new(false);
            assert_eq!({{name}}.get(), false);
            {{name}}.flip();
            assert_eq!({{name}}.get(), true);
        }
    }
}
