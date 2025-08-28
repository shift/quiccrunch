fn main() {
    #[cfg(feature = "backend-tquic")]
    println!("tQUIC backend is enabled");
    
    #[cfg(feature = "backend-quinn")]
    println!("Quinn backend is enabled");
    
    #[cfg(not(any(feature = "backend-tquic", feature = "backend-quinn")))]
    println!("No backend is enabled");
    
    let backend_name = {
        #[cfg(feature = "backend-tquic")]
        {
            "tQUIC"
        }
        #[cfg(all(feature = "backend-quinn", not(feature = "backend-tquic")))]
        {
            "Quinn"
        }
        #[cfg(not(any(feature = "backend-tquic", feature = "backend-quinn")))]
        {
            "Unknown"
        }
    };
    
    println!("Detected backend: {}", backend_name);
}