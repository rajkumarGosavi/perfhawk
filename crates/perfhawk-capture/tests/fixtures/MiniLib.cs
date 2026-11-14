// Source of MiniLib.dll (netstandard2.0, IL only), the metadata fixture for tests/r2r.rs.
// Rebuild: a one-file netstandard2.0 class library project with Deterministic=true and DebugType=none.
namespace Mini
{
    public class Outer
    {
        public void Run() { }
        public void Stop() { }

        public class Inner
        {
            public void Deep() { }
        }
    }

    public class Box<T>
    {
        public T Get() => default!;
    }
}

public class TopLevel
{
    public void Global() { }
}
