fn interop_example() {
    let x: i32 = 42;
    {
        use std :: collections :: HashMap ; let mut map = HashMap :: new ( ) ; map . insert ( "answer" , x ) ; println ! ( "{:?}" , map ) ;
    }
}
