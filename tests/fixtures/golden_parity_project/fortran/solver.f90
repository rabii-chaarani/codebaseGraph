module math_mod
use iso_fortran_env
contains
subroutine greet()
call print_hello()
end subroutine greet
function add(a,b) result(c)
integer :: a,b,c
c = a+b
end function add
end module math_mod
