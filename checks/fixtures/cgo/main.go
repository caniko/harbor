package main

/*
static int answer(void) { return 42; }
*/
import "C"

import "fmt"

func answer() int { return int(C.answer()) }

func main() { fmt.Println(answer()) }
