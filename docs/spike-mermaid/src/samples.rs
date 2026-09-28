pub const DIAGRAMS: &[(&str, &str)] = &[
("flowchart", r#"flowchart TD
    A[Open file] --> B{Is markdown?}
    B -- yes --> C[Parse fences]
    B -- no --> D[Code editor]
    subgraph Render [Mermaid render]
        C --> E[Hash source]
        E --> F{In cache?}
        F -- hit --> G[Show image]
        F -- miss --> H[Background render]
        H --> I[Rasterize SVG]
        I --> G
    end
    D --> J((Done))
    G --> J
"#),
("sequence", r#"sequenceDiagram
    participant U as User
    participant E as Explore
    participant B as Background
    participant C as Cache
    U->>E: open README.md
    E->>C: lookup(hash)
    alt cache hit
        C-->>E: image
    else miss
        E->>B: render(source)
        loop each diagram
            B->>B: layout + svg
        end
        Note over B,C: rasterize at scale factor
        B-->>C: store
        C-->>E: image
    end
    Note right of U: diagram visible
    E-->>U: repaint
"#),
("class", r#"classDiagram
    class Explore {
        +String path
        +Vec~Segment~ segments
        +render() Element
    }
    class Segment {
        <<enumeration>>
        Markdown
        Mermaid
    }
    class DiagramCache {
        -HashMap~u64, Arc~Image~~ map
        +get(hash) Option
        +insert(hash, image)
    }
    Explore "1" --> "*" Segment
    Explore --> DiagramCache : uses
    Animal <|-- Duck
    Animal : +int age
    Animal : +mate()
"#),
("state", r#"stateDiagram-v2
    [*] --> Idle
    Idle --> Rendering : source changed
    Rendering --> Ready : ok
    Rendering --> Failed : parse error
    Failed --> Rendering : edit
    Ready --> Rendering : theme changed
    state Rendering {
        [*] --> Parse
        Parse --> Layout
        Layout --> Svg
        Svg --> [*]
    }
    Ready --> [*]
"#),
("er", r#"erDiagram
    CUSTOMER ||--o{ ORDER : places
    ORDER ||--|{ LINE_ITEM : contains
    CUSTOMER {
        string name
        string email
    }
    ORDER {
        int id
        date created
    }
"#),
("pie", r#"pie title Diagram types
    "Flowchart" : 45
    "Sequence" : 30
    "Class" : 15
    "Other" : 10
"#),
("gantt", r#"gantt
    title Mermaid preview
    dateFormat YYYY-MM-DD
    section Research
    Survey renderers :done, a1, 2026-09-01, 5d
    Spike            :active, a2, after a1, 3d
    section Build
    Integrate        :b1, after a2, 7d
    Theme            :b2, after b1, 2d
"#),
];
