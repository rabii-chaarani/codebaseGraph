package main

import "fmt"

type User struct {
	ID int
}

func (u User) Name() string {
	fmt.Println(u.ID)
	return ""
}

func helper() {
	fmt.Println(1)
}
