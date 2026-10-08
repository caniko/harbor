package main

import "testing"

func TestCCompiler(t *testing.T) {
	t.Parallel()
	if got := answer(); got != 42 {
		t.Fatalf("answer() = %d, want 42", got)
	}
}
