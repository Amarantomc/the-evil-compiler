# Compilador de HULK

Este documento describe la arquitectura interna del compilador de **HULK** implementado en Rust: las decisiones de diseño que lo sustentan, las características del lenguaje que soporta, los mecanismos concretos con los que cada fase está resuelta y las limitaciones conocidas. El objetivo es que sirva como referencia completa del proyecto: cualquier persona que lo lea debería entender *qué* hace cada componente, *por qué* está hecho así, *cómo* está implementado a nivel de código y *dónde* están los límites de la implementación.

El compilador es un **compilador completo a código nativo**: toma un archivo fuente `.hulk`, lo analiza (léxico, sintáctico y semántico), genera **LLVM IR** (`output.ll`) y finalmente produce un ejecutable nativo (`./output`) para Linux x86-64 enlazando contra `libc`/`libm` mediante `clang` (o `llc` + `cc` como alternativa). No interpreta ni usa una máquina virtual intermedia: el resultado final es un binario ejecutable real.

### Por qué Rust y por qué LLVM

Dos decisiones tecnológicas atraviesan todo el proyecto. La primera es **Rust** como lenguaje de implementación. Su sistema de tipos algebraicos (`enum` con *payload*) modela un AST de forma natural y segura: cada forma de expresión es una variante de un `enum`, y el *pattern matching* exhaustivo del compilador de Rust garantiza que ningún caso quede sin tratar cuando se añade una variante nueva. El *borrow checker*, además, fuerza a ser explícito sobre la mutabilidad del AST, lo cual importa porque varias fases (promoción, monomorfización, inferencia) **reescriben** el árbol en sitio.

La segunda decisión es **emitir LLVM IR como texto** en lugar de usar un *binding* a la API de LLVM (como `inkwell`). Generar IR textual hace que el *backend* sea autocontenido, fácil de inspeccionar (basta abrir `output.ll`), independiente de tener enlazada la biblioteca de LLVM en tiempo de compilación del propio compilador, y trivial de depurar: cuando algo falla, el IR generado es legible y se puede pasar a `clang`/`llc` a mano. El coste es que el compilador no aprovecha las verificaciones que la API de LLVM haría sobre el IR en memoria; a cambio, delega la optimización y el *lowering* a `clang -O2`.

---

## 1. Visión general del *pipeline*

El punto de entrada (`main.rs`) orquesta una tubería de fases bien delimitadas. El orden importa porque cada fase asume **invariantes** establecidas por la anterior, y romper ese orden produciría errores internos en lugar de diagnósticos limpios:

1. **Lectura del fuente.** Se lee el archivo pasado como argumento. Un fallo de E/S aquí se reporta como error léxico (es lo más temprano que puede fallar).
2. **Análisis léxico + sintáctico.** Un *lexer* propio (basado en expresiones regulares) alimenta a un parser **LALR(1)** generado con **LALRPOP**. El resultado es un AST (`Program`). Un único error de esta etapa se reporta clasificándolo como LÉXICO o SINTÁCTICO.
3. **Genéricos.** Se ejecuta `promote::promote_program` y después el `Monomorphizer`. Esta fase reescribe el AST: distingue parámetros genéricos de clases reales y luego elimina las plantillas genéricas sustituyéndolas por instancias concretas (monomorfización).
4. **Detección de herencia circular.** `semantic::detect_inheritance_cycles` recorre la cadena de ancestros de cada tipo y detecta ciclos antes de continuar, porque toda la inferencia y el *codegen* posteriores asumen que la jerarquía de herencia es un árbol (sin ciclos).
5. **Inferencia de tipos.** `TypeInferrer::infer_program` recorre el AST, genera restricciones, las resuelve por unificación y **anota** cada nodo con su `HulkType` concreto.
6. **Chequeo semántico.** `SemanticChecker::check_program` recibe el AST ya anotado y verifica todas las reglas del lenguaje (conformidad de tipos, aridad, existencia de miembros, validez de `self`/`base`, etc.).
7. **Generación de código.** `codegen::compile_hulk_program` emite el LLVM IR a `output.ll`.
8. **Enlazado.** `build_output` invoca a `clang` para producir `./output`; si no está disponible, recurre a `llc` + `cc`.

Cada fase se aborta limpiamente ante errores acumulados, y el código de salida del proceso codifica la fase que falló. Es importante notar que las fases 3–6 conforman, en conjunto, la **fase semántica** del contrato: aunque internamente son cuatro pasos distintos (monomorfización, ciclos, inferencia, chequeo), cualquier error en cualquiera de ellos se reporta con el código semántico (3). Esto refleja una decisión consciente: para el usuario del compilador, todo lo que ocurre después de tener un AST sintácticamente válido es "análisis semántico".

### Por qué este orden y no otro

El orden no es arbitrario. La monomorfización va **antes** que la inferencia porque la inferencia opera sobre tipos concretos: si dejásemos plantillas genéricas sin instanciar, el inferidor tendría que razonar sobre tipos parametrizados, lo que complicaría enormemente la unificación. Al monomorfizar primero, el inferidor solo ve código completamente concreto. La detección de ciclos de herencia va antes de la inferencia porque la propia inferencia recorre cadenas de ancestros (para calcular conformidad y LCA), y un ciclo la haría recursar infinitamente. Y la inferencia va antes del chequeo porque el chequeo **lee** tipos en lugar de calcularlos: necesita que el AST ya esté anotado.

### Contrato de diagnósticos y códigos de salida

El módulo `errors.rs` implementa el contrato de la interfaz del compilador (el "HULK Compiler Interface Contract"). Los mensajes tienen el formato exacto `(line,col) TYPE: message`, con línea y columna **1-based** (y `(0,0)` cuando no hay posición disponible). Los códigos de salida son: **1 = LEXICAL**, **2 = SYNTACTIC**, **3 = SEMANTIC** y **0 = éxito**.

La pieza clave de traducción es `from_parse_error`, que convierte los errores de LALRPOP al contrato. LALRPOP entrega un único `ParseError` con varias variantes, y la función las desambigua así: un `ParseError::User` proviene del lexer externo (porque LALRPOP envuelve los errores del lexer en esa variante) y se clasifica como **LÉXICO**; el resto de variantes (`InvalidToken`, `UnrecognizedToken`, `UnrecognizedEof`, `ExtraToken`) son errores de la gramática y se clasifican como **SINTÁCTICO**. Cada una construye un mensaje legible: para `UnrecognizedToken` se describe el token ofensor con `describe()` y se listan las alternativas esperadas con `fmt_expected()`, lo que produce mensajes del estilo "token inesperado 'X'; se esperaba ...". La función `line_col` convierte un *offset* de byte a coordenadas de línea/columna recorriendo el fuente carácter a carácter y contando saltos de línea; es la que traduce las posiciones internas (que siempre son *offsets* de byte) al formato del contrato.

Esta separación de responsabilidades —el compilador trabaja internamente con *offsets* de byte, y solo en el último momento se traducen a línea/columna— evita arrastrar coordenadas de texto por todo el AST y mantiene los nodos ligeros.

---

## 2. Análisis léxico

El lexer (`lexer/lexer.rs`) está **escrito a mano** sobre expresiones regulares ancladas (`^`), y produce tripletas `(inicio, Token, fin)` que LALRPOP consume como un lexer externo (es decir, implementa el *trait* `Iterator` de Rust con `Item = Result<(usize, Token, usize), LexicalError>`, exactamente el contrato que LALRPOP espera de un lexer externo).

### Maximal munch determinista por orden de reglas

La decisión central del lexer es resolver el *maximal munch* (la regla de "casar el lexema más largo posible") **mediante el orden de la lista de reglas**, no mediante longitud calculada. Las reglas se almacenan en un `Vec<(Regex, Kind)>` y se prueban en orden; la primera que casa gana. Por eso los patrones multi-carácter se colocan *antes* que los de un carácter: `@@` antes que `@`, `:=` antes que `:`, `==` antes que `=`, `<=` antes que `<`, y el flotante `[0-9]+\.[0-9]+` antes que el entero `[0-9]+`. **El orden de la lista *es* la prioridad.**

Este enfoque es más simple y predecible que calcular la longitud de todos los lexemas candidatos y quedarse con el más largo: el autor del lexer controla la desambiguación leyendo la lista de arriba abajo. Todas las regex están ancladas con `^` para que casen solo al inicio del resto de la entrada, lo que las convierte en efectivamente "casar aquí o no casar".

### Promoción de palabras clave

En vez de tener una regex por cada palabra reservada, el lexer reconoce primero un identificador genérico (`[A-Za-z_][A-Za-z0-9_]*`) y después la función `keyword()` decide si ese texto es realmente una palabra reservada (`if`, `let`, `type`, `function`, `inherits`, `new`, `base`, `self`, `is`, `as`, `Number`, `String`, `Boolean`, …). Si lo es, devuelve el *token* de keyword correspondiente; si no, el texto se convierte en `Token::Ident`. Esto mantiene el lexer compacto (una sola regla de identificador en lugar de ~20 reglas de keyword) y evita el clásico error de que un identificador como `ifx` se reconozca parcialmente como `if`: la regla de identificador casa el lexema completo `ifx`, y solo después se comprueba contra la tabla de keywords (que no contiene `ifx`).

### Comentarios y espacios

Antes de intentar casar un token real, el lexer entra en un bucle que salta espacios en blanco y comentarios consecutivos. Soporta comentarios de línea (`// …`, hasta el siguiente salto de línea o el fin de archivo) y de bloque (`/* … */`). Un comentario de bloque sin cerrar produce un **error léxico explícito** con la posición de apertura, en lugar de consumir silenciosamente el resto del archivo: es un caso que el lexer detecta y reporta deliberadamente.

### Cadenas con escapes

Las cadenas se reconocen con una regex que admite escapes (`"(?:\\[\s\S]|[^"\\])*"`) y luego se decodifican con `unescape_string`, que traduce las secuencias `\n`, `\t`, `\r`, `\\`, `\"`, `\'` y `\0` a sus caracteres reales. Un escape no reconocido produce un error léxico **ubicado en el carácter ofensor**: `unescape_string` devuelve el *offset* relativo del `\` problemático dentro del contenido, y el lexer lo suma a la posición de apertura de la cadena para dar una columna exacta. Este nivel de precisión en los errores es una característica de calidad: el usuario sabe exactamente qué escape falló.

### Posiciones precisas y el detalle del `f32`

Cada token lleva su rango de bytes `(inicio, fin)`, lo que permite a las fases posteriores ubicar errores con exactitud. Los `Token` (en `lexer/token.rs`) cubren literales con *payload* (`Int(usize)`, `Float(f32)`, `Str(String)`, `Ident(String)`), todas las palabras clave y todos los operadores/puntuación del lenguaje.

Conviene señalar aquí un detalle de precisión numérica: los enteros se almacenan en `Int(usize)` y los flotantes en `Float(f32)`. La gramática unifica ambos en el tipo de la regla `Num`, que devuelve `f32` (convirtiendo el `usize` con `as f32`). Como el *backend* de LLVM trabaja con `double` (`f64`), hay una conversión `f32 → f64` implícita al emitir literales. Para la mayoría de los programas esto es irrelevante, pero técnicamente significa que un literal numérico no se representa con la precisión completa de un `double` durante el *pipeline* del compilador; es una limitación menor que se documenta en la sección 11.

---

## 3. Análisis sintáctico

La gramática vive en `grammar.lalrpop` y se compila con **LALRPOP** a un parser LALR(1). Usa el bloque `extern { ... }` para conectarse al lexer propio: ahí se declara el tipo de las posiciones (`type Location = usize`), el tipo de error (`type Error = LexicalError`) y el mapeo de cada *token* del lexer a un terminal de la gramática (`"ident" => Token::Ident(<String>)`, etc.).

### Por qué un lexer externo

LALRPOP puede generar su propio lexer interno a partir de literales de cadena en la gramática, pero el proyecto usa un lexer externo deliberadamente. El lexer interno de LALRPOP no ofrece con la misma comodidad el control fino que aquí se necesita: comentarios de línea y de bloque, decodificación de secuencias de escape en cadenas, errores léxicos con posición exacta, y el *maximal munch* explícito por orden de reglas. Tener el lexado y el parseo **desacoplados** permite que cada uno haga lo que mejor sabe: el lexer maneja el nivel de caracteres, y la gramática solo razona sobre *tokens*.

### Precedencia de operadores

La precedencia se codifica como una **cascada de reglas**: cada nivel delega al siguiente, de menor a mayor prioridad. El truco es que cada regla es recursiva por la izquierda en su propio operador y referencia el siguiente nivel para los operandos, lo que produce naturalmente la asociatividad izquierda y la precedencia ascendente sin necesidad de declaraciones de precedencia explícitas (`%left`, `%right`). La cascada es:

```
Or (| ||) → And (& &&) → Assign (:=) → Comp (== != >= <= > <)
   → Conc (@ @@) → Is (is) → As (as) → Arith (+ -) → FactAnd (* / %)
   → Unary (+ - !) → Pow (^) → Term
```

Un detalle interesante de esta tabla es la posición de la **asignación destructiva** `:=`: está entre `And` y `Comp`, es decir, tiene precedencia muy baja (solo por encima de los conectores lógicos). Esto significa que `x := a + b` parsea como `x := (a + b)`, lo esperado, pero `a && x := b` parsea como `a && (x := b)`. La regla de asignación es recursiva por la derecha (`<id:Term> ":=" <expr:AssignExpr>`), de modo que `a := b := c` asocia a la derecha como `a := (b := c)`.

`Term` es el nivel atómico: literales numéricos/booleanos/de cadena (con su *span* capturado vía `<l:@L> ... <r:@R>`), `new T(...)` y `new T::<Args>(...)`, accesos con punto (`x.campo`, `x.metodo(...)`, `x.0` para tuplas), llamadas a función, construcción de tuplas `(a, b, ...)`, expresiones entre paréntesis, `base(...)`, `self` e identificadores. Los tres tipos de acceso con punto se distinguen sintácticamente: `.` seguido de un entero es acceso a tupla, `.` seguido de una llamada es invocación de método, y `.` seguido de un identificador es acceso a campo.

### El "problema del cuerpo abierto" (OpenS / OpenB)

HULK es un lenguaje **orientado a expresiones**: `if`, `while`, `for` y `let … in` no son sentencias, sino expresiones que devuelven un valor, y su cuerpo puede ser tanto una expresión simple como un bloque `{ … }`. Esto introduce una ambigüedad análoga al clásico *dangling else*. Considérese `if (c) a else b + 1`: ¿el `+ 1` forma parte del cuerpo `else` (dando `if (c) a else (b + 1)`) o el `if` entero es el operando izquierdo de un `+` (dando `(if (c) a else b) + 1`)? Sin una regla, esto es un conflicto *shift/reduce* en la tabla LALR.

La gramática lo resuelve **duplicando** las cadenas de precedencia en dos variantes paralelas. La variante `…OpenS` ("simple") describe expresiones cuyo cuerpo de control termina en una expresión simple, y la variante `…OpenB` ("bloque") describe aquellas cuyo cuerpo termina en un bloque `{ … }`. Hay, por tanto, tres copias de buena parte de la cascada: la normal (`OrExpr`, `AndExpr`, …), la `OpenS` (`OrExprOpenS`, …) y la `OpenB` (`OrExprOpenB`, …). Las reglas auxiliares `BodyS` y `BodyB` controlan qué puede aparecer como cuerpo de cada constructo, y las reglas `OpenExprS`/`OpenExprB` son las que efectivamente construyen los nodos `If`/`While`/`For`/`Let`.

El resultado es que el parser sabe de forma **determinista** dónde termina cada cuerpo de control, sin conflictos en la tabla LALR y sin recurrir a precedencias artificiales. El coste es la verbosidad: la cascada está triplicada, y cualquier cambio en la precedencia debe replicarse en las tres variantes. Es un compromiso consciente entre limpieza de la tabla LALR y mantenibilidad del archivo de gramática.

### Anotaciones de tipo y genéricos en la sintaxis

La regla `Type` reconoce los tipos primitivos (`Number`, `String`, `Boolean`), las clases (un `Ident` se convierte en `HulkType::Class`), los tipos genéricos aplicados (`Id<T, U>` → `HulkType::Generic`) y las tuplas (`(T1, T2, …)` → `HulkType::Tuple`, que exige al menos dos elementos por la coma obligatoria). Los parámetros genéricos de declaración se capturan con `GenericParams` (`<T, U>`) y la instanciación explícita usa la sintaxis *turbofish* `::<...>` (regla `TurboArgs`), tanto en llamadas a función como en `new`. La elección del *turbofish* (en lugar de `Id<T>` directamente en posición de expresión) evita la ambigüedad clásica entre `<` como operador de comparación y `<` como apertura de lista de argumentos de tipo, el mismo problema que llevó a Rust a adoptar esa misma sintaxis.

---

## 4. El AST y el patrón Visitor

El núcleo del AST está en `nodes/expr_node.rs`. La enumeración `Expr` representa **todas** las formas de expresión: literales, operaciones binarias/unarias, `let`, `if`, `while`, `for`, llamadas a función, instanciación (`new`), acceso a miembros y métodos, `self`, `base`, *downcast* (`as`), *type test* (`is`), construcción de tuplas y acceso a tupla. Cada nodo concreto vive en su propio archivo dentro de `nodes/` (por ejemplo `if_node.rs`, `let_node.rs`, `tuple_node.rs`) y la mayoría guarda un campo `return_type: HulkType` que se rellena durante la inferencia. Separar cada nodo en su archivo mantiene los módulos pequeños y hace evidente qué datos lleva cada forma de expresión.

### El sistema de tipos `HulkType`

El sistema de tipos del lenguaje se modela con un único `enum`:

```
Number | Bool | String | Class(nombre) | Tuple(Vec<HulkType>)
       | Param(nombre) | Generic(nombre, args) | Unknown
```

Las tres primeras variantes son los primitivos. `Class` es un tipo nominal definido por el usuario. `Tuple` es un producto de tipos heterogéneos. Las tres últimas variantes existen para soportar las características avanzadas: `Param` representa un parámetro genérico libre (la `T` de una declaración genérica), `Generic` un tipo genérico aplicado pendiente de monomorfizar (`Box<Number>`), y `Unknown` un tipo que aún no se ha inferido o que el inferidor no pudo determinar.

`HulkType` no es solo un dato pasivo: ofrece las operaciones que hacen posibles los genéricos. `promote_params(generics)` convierte `Class(T)` en `Param(T)` para cada nombre `T` que aparezca en la lista de parámetros genéricos, recursando en tuplas y genéricos aplicados (es el corazón de la fase de promoción). `subst(map)` sustituye cada `Param(n)` por el tipo concreto del mapa, también recursivamente (es el corazón de la especialización). `mangle()` produce un nombre plano apto para identificadores LLVM (`Tup2_double_ptr`, `Box__Number`, etc.). `collapse_generic()` colapsa un `Generic` ya concreto a `Class("nombre_manglado")` después de la sustitución. Y `contains_param()` indica si un tipo todavía contiene parámetros libres, lo que se usa para decidir si una instancia genérica puede resolverse ya o debe posponerse. Estas cinco operaciones, definidas una sola vez sobre `HulkType`, son reutilizadas por la promoción, la monomorfización y la inferencia.

### El patrón Visitor compartido

El recorrido del AST se hace con el patrón **Visitor**. El *trait* `ExprVisitor<T>` (en `expr_visitor.rs`) define un método `visit_*` por cada forma de expresión, y `Expr::accept` despacha a su método correspondiente vía *pattern matching* sobre la variante. El parámetro de tipo `T` es el tipo del resultado que produce cada visita.

Lo elegante de este diseño es que **tanto el inferidor de tipos como el generador de código son visitantes del mismo *trait***: `TypeInferrer` implementa `ExprVisitor<InferType>` (cada visita devuelve el tipo inferido de la subexpresión) y `CodeGenerator` implementa `ExprVisitor<GeneratorResult>` (cada visita devuelve el registro LLVM y el tipo LLVM donde quedó el valor). Una sola estructura de recorrido —el *dispatch* de `accept`— sirve a dos fases radicalmente distintas, sin duplicar la lógica de "cómo se recorre el árbol". Añadir una nueva forma de expresión obliga, gracias a la exhaustividad de Rust, a implementar su visita en ambos sitios, lo que evita olvidos.

### Posiciones (*spans*)

Las posiciones se llevan en los literales (provienen del lexer, capturadas en la gramática con `@L`/`@R`) y, para los nodos compuestos, se derivan estructuralmente con `Expr::span`, que combina los *spans* de las hojas (típicamente el del operando más a la izquierda). Cuando no hay posición disponible se reporta `(0,0)`, valor que el contrato permite. Esto da diagnósticos semánticos ubicados sin necesidad de almacenar un *span* explícito en cada nodo.

---

## 5. Característica adicional: Genéricos (monomorfización)

Los genéricos se implementan por **monomorfización**, al estilo de las plantillas de C++ o los genéricos de Rust: por cada combinación concreta de argumentos de tipo se genera una copia especializada del código. La alternativa habitual (genéricos por borrado de tipos, como en Java) se descartó porque obligaría a un tratamiento uniforme de todos los valores (típicamente vía *boxing*), mientras que la monomorfización produce código especializado y sin coste en tiempo de ejecución. La implementación ocurre en dos pasos: promoción y monomorfización propiamente dicha.

### Paso 1 — Promoción (`generics/promote.rs`)

El problema que resuelve la promoción es de **identidad**: dentro de la declaración `type Box<T> { value : T; }`, el nombre `T` se parsea como `HulkType::Class("T")`, indistinguible de una clase real llamada `T`. La promoción reescribe, dentro de cada declaración genérica, los nombres de tipo que coinciden con un parámetro declarado, convirtiéndolos de `Class(T)` a `Param(T)`. Usa el método `promote_params` de `HulkType`, que recursa sobre tuplas y genéricos aplicados, de modo que un campo de tipo `(T, Number)` se promueve correctamente a `Tuple([Param(T), Number])`. La promoción cubre todos los lugares donde puede aparecer una anotación de tipo: parámetros de constructor, atributos, firmas de método (parámetros y tipo de retorno), anotaciones de `let` y argumentos *turbofish*.

### Paso 2 — Monomorfización (`generics/mono.rs`)

El `Monomorphizer` trabaja con una *worklist* hasta alcanzar un punto fijo. Su algoritmo `run` tiene cuatro etapas:

1. **Separación.** Recorre los *statements* del programa y separa las **plantillas** (funciones y tipos cuyo campo `generics` no está vacío, detectado con `is_generic()`) del resto, las "raíces" concretas (funciones/tipos no genéricos y expresiones de nivel superior). Las plantillas se guardan en `fn_templates` y `type_templates`; las raíces, en `roots`.

2. **Siembra de la *worklist*.** Recorre las raíces buscando usos de plantillas con argumentos de tipo concretos, vía `scan_expr`. Cuando encuentra una llamada o un `new` cuyo nombre es el de una plantilla y que lleva `type_args` no vacíos, calcula el nombre **manglado** (`mangle_name`, p. ej. `Box__Number`), encola la instancia `(base, args)` en la cola, **reescribe el nombre del nodo** a su forma manglada y vacía sus `type_args`. Así, después de esta pasada, las raíces ya no referencian plantillas por su nombre genérico sino por el nombre de la instancia concreta.

3. **Punto fijo.** Procesa la cola: por cada instancia pendiente que no esté ya hecha (lleva un conjunto `done` para no repetir trabajo), recupera la plantilla y la **especializa** sustituyendo `Param` por los tipos concretos (`specialize_fn` para funciones, `specialize_type` para tipos, ambas apoyadas en `subst_expr`, que propaga la sustitución por el cuerpo). Tras especializar, **vuelve a escanear** el cuerpo especializado, porque puede contener usos de *otras* plantillas (genéricos anidados): por ejemplo, `Box<Box<Number>>` siembra primero `Box__Number` y, al especializarlo, descubre `Box__Box__Number`. El conjunto `done` garantiza terminación incluso con genéricos recursivos acotados.

4. **Reensamblado.** Reconstruye el programa colocando primero los tipos concretos (`out_types`), luego las funciones (`out_fns`) y después el resto de raíces (`kept`). Este orden no es cosmético: el generador de código espera procesar los tipos antes que las funciones, porque las funciones pueden instanciar tipos.

### Mangling y genéricos anidados

El *mangling* (`mangle_name`) produce nombres planos uniendo el nombre base con los argumentos manglados separados por `__` y `_` (p. ej. `Box__Number`, `Pair__Number_String`), y las tuplas se codifican como `TupN_…`. Hay un caso sutil que `scan_expr` maneja explícitamente: si los argumentos de tipo de un uso todavía contienen `Param` (porque ese uso está dentro de *otra* plantilla aún no resuelta), la instancia **no se encola**; se pospone, y la resolverá la especialización del padre cuando esta sustituya los `Param` por tipos concretos vía `subst_expr`. Esto es lo que permite que los genéricos anidados se resuelvan en el orden correcto.

### Decisión de diseño y su coste

La monomorfización da generación de código sencilla y sin coste en tiempo de ejecución (no hay despacho dinámico ni *boxing* impuesto por los genéricos), a cambio de dos cosas: posible *code bloat* (cada combinación de argumentos genera código nuevo) y la exigencia de **instanciación explícita por *turbofish***. Este compilador no infiere los argumentos de tipo desde el uso: si se quiere `Box<Number>`, hay que escribir `new Box::<Number>(...)`. Inferir los argumentos de tipo requeriría integrar la resolución de genéricos con la inferencia de tipos, lo que aumentaría notablemente la complejidad; el *turbofish* obligatorio es el precio de mantener la monomorfización como una fase independiente y anterior a la inferencia.

---

## 6. Característica adicional: Tuplas

Las tuplas tienen nodos propios (`nodes/tuple_node.rs`): `TupleNode` para la construcción `(e1, e2, …)` y `TupleAccessNode` para el acceso por índice `expr.N`. La justificación de no reutilizar `BlockNode` ni `FunCallNode` es semántica: una tupla es un **producto de valores heterogéneos** con índice numérico de acceso, conceptualmente distinto de una secuencia de sentencias (bloque) o de una llamada. Modelarla con su propio nodo mantiene la semántica explícita en el AST.

- **Tipo.** Se modelan como `HulkType::Tuple(Vec<HulkType>)`. El índice de acceso es un `usize` conocido en tiempo de parseo (`p.0`, `p.1`, sintácticamente un entero tras el punto), lo que simplifica tanto la verificación (basta comprobar `index < len`) como la generación de código (un GEP directo con el índice como constante).
- **Inferencia.** El acceso a tupla genera una restricción especial `TupleProject(tipo_tupla, índice, resultado)`. Mientras el tipo de la tupla siga siendo una variable, la restricción se reencola; cuando se resuelve a un `Tuple` concreto, el *solver* extrae el tipo del elemento en esa posición y lo enlaza al resultado. Es una de las tres clases de restricción del inferidor, junto a `Eq` y `Conform`.
- **Código.** En LLVM se representan como *structs* anónimos con nombre derivado de sus elementos (`%Tuple_double_ptr_…`, generado por `tuple_struct_name`), emitidos bajo demanda con `ensure_tuple_type_emitted` (para no declarar dos veces el mismo tipo de tupla). La construcción reserva el *struct*, almacena cada elemento por GEP y carga el valor; el acceso almacena la tupla en un temporal, hace GEP del índice y carga el elemento.
- **Verificación.** El *checker* valida que el índice esté dentro de rango y que el acceso se haga sobre un tipo tupla y no sobre otro tipo.

---

## 7. Características adicionales: `is` y `as`

`is` (*type test*) y `as` (*downcast*) son operadores de **identificación y conversión de tipos en tiempo de ejecución**, con un mecanismo de RTTI (información de tipos en *runtime*) muy económico.

**Semántica (`is`).** `expr is T` devuelve `Boolean`. El *checker* exige que `expr` sea de tipo clase (o `Unknown`, para permitir programas parcialmente inferidos).

**Semántica (`as`).** `expr as T` tiene como tipo estático la clase destino `T`. El *checker* aplica tres reglas: (1) la fuente debe ser de tipo clase; (2) el tipo destino debe estar declarado; (3) fuente y destino deben estar **relacionados** por herencia (uno debe ser ancestro del otro). Un *downcast* entre tipos no relacionados es un error semántico estático, detectado antes de generar código.

**Implementación en tiempo de ejecución (RTTI por identidad de *vtable*).** La clave está en que **cada clase concreta tiene una *vtable* única** (`@vtable_T`). Como el puntero a *vtable* es el campo 0 de todo objeto, la identidad de tipo de un objeto en *runtime* se reduce a la identidad de su puntero de *vtable*. Para `x is T`, el generador carga el puntero a *vtable* del objeto (GEP al campo 0, luego `load`), recolecta todos los subtipos de `T` con `collect_subtypes` (que recorre `class_meta` siguiendo las relaciones de herencia registradas) y compara la *vtable* del objeto contra `@vtable_S` de cada subtipo `S` con `icmp eq ptr`. Las comparaciones se combinan con una cadena de `or i1`. El resultado es un `i1`. Si `T` no está en `class_meta` (no es una clase conocida), el resultado es siempre falso.

Para `x as T` se hace exactamente la misma comprobación de conformidad y se ramifica con `br i1`: si conforma, el control salta a un bloque `cast_ok` que devuelve el mismo puntero pero ahora con el tipo estático destino `T`; si no, salta a `cast_fail`, que llama a `@hulk_cast_error()` (una función que imprime un mensaje de error de *runtime* con `printf` y luego `abort`) seguido de `unreachable`. El `unreachable` informa a LLVM de que ese camino no retorna, lo que ayuda a la optimización.

Este enfoque tiene una virtud notable: **no necesita etiquetas de tipo separadas ni cabeceras de objeto adicionales**. Reutiliza la *vtable* que ya existe para el despacho dinámico como identidad de tipo, de modo que el RTTI sale "gratis" del mecanismo de herencia. El coste es que `is`/`as` son lineales en el número de subtipos del tipo destino (se compara contra cada uno) y solo funcionan sobre clases, no sobre primitivos ni tuplas.

---

## 8. Característica adicional: Inferencia de tipos

La inferencia (`type_inferrer.rs`, el módulo más grande del proyecto) sigue un enfoque **basado en restricciones** con sabor Hindley-Milner, organizado en cuatro etapas. Su responsabilidad está acotada de forma muy deliberada, como se explica más abajo.

### Tipos internos y restricciones

Durante la inferencia, los tipos se representan con `InferType`, que tiene solo dos variantes: `Concrete(HulkType)` (un tipo conocido) y `Var(u32)` (una **variable de tipo fresca**, un tipo todavía desconocido identificado por un número). Cada posición sin anotar del programa recibe una `Var` nueva, generada por `VarGen`.

Las restricciones son de tres clases: `Eq(a, b)` exige igualdad exacta (unificación); `Conform(sub, sup)` exige que `sub` sea subtipo de `sup` (subtipado nominal); y `TupleProject(tupla, idx, resultado)` exige que `resultado` sea el tipo del elemento `idx` de `tupla`. Esta última es específica de las tuplas y permite inferir el tipo de un acceso `p.0` aunque el tipo de `p` aún no se conozca en el momento de generar la restricción.

### Las cuatro etapas

1. **Registro de declaraciones.** Se construyen las firmas estáticas: para cada tipo, su `TypeInfo` (nombre, parámetros de constructor, campos, métodos, padre); para cada función, su firma. Las posiciones sin anotar reciben variables frescas vía `from_annotation`, que convierte una anotación `Unknown` en una `Var` nueva y cualquier otra anotación en su `Concrete` correspondiente.
2. **Generación de restricciones.** Un recorrido *bottom-up* del AST, usando el `ExprVisitor`, produce restricciones a partir del uso. Por ejemplo, una suma `a + b` genera `Eq(tipo_a, Number)` y `Eq(tipo_b, Number)`; una asignación genera `Conform(tipo_valor, tipo_variable)`; un `if`/`elif`/`else` calcula el tipo de cada rama y reconcilia; etc.
3. **Resolución iterativa.** El *solver* (`solve_constraints`) usa una *worklist* (`VecDeque`) y una **sustitución de union-find plano** (`Substitution`). `Substitution::apply` sigue la cadena de enlaces de una variable hasta un tipo concreto o una variable sin mapear; `bind` registra `var → tipo` sin pisar un *binding* concreto previo. El *solver* procesa cada restricción con `process_constraint`: las `Eq` con una `Var` enlazan la variable; las `TupleProject` con tupla concreta proyectan el elemento; las restricciones que aún no pueden resolverse (porque dependen de variables sin resolver) se **reencolan**. Para no ciclar indefinidamente, el *solver* detecta el **estancamiento** (*stall*): lleva un contador `stalled_count` que se reinicia cada vez que algo cambia y, si supera el tamaño de la cola, concluye que las restricciones restantes son irresolubles y termina.
4. **Anotación del AST.** Cada `Var` se resuelve a su tipo concreto con `resolve` (una `Var` sin resolver se anota como `Unknown`) y se fija el `return_type` de cada nodo. Para `if`/`elif`/`else` se calcula el **ancestro común más bajo (LCA)** de las ramas vía `Environment::lca`, de modo que el tipo de un condicional sea el supertipo común de sus ramas (por ejemplo, si una rama devuelve `Cat` y otra `Dog`, ambos descendientes de `Animal`, el `if` tiene tipo `Animal`).

### Decisión de diseño central: inferir vs. verificar

El inferidor **no emite errores semánticos**. Solo registra errores estructurales irrecuperables que le impiden continuar (por ejemplo, una función no declarada al generar restricciones). Todos los conflictos de tipo reales —un `Number` donde se esperaba `String`, una conformidad que no se cumple— se dejan pasar **deliberadamente** para que el `SemanticChecker` los detecte sobre el AST ya anotado. Esto se ve en `process_constraint`: cuando una restricción `Eq` o `Conform` tiene ambos lados concretos y son incompatibles, el inferidor **no falla**; simplemente da la restricción por procesada y deja el conflicto en pie. La separación es limpia: el inferidor *infiere* (propaga información de tipos por el árbol), y el chequeador *verifica* (comprueba que esa información es coherente con las reglas del lenguaje). Tener dos responsabilidades en dos módulos distintos hace cada uno más simple y testeable.

### El entorno

El `Environment` implementa la relación de **conformidad nominal**. `conforms_concrete(sub, sup)` decide si `sub` conforma a `sup`: trata `Unknown` y `Param` como conformes con cualquier cosa (permisividad deliberada), compara genéricos por su forma manglada, rechaza cualquier mezcla con primitivos distintos, y para dos clases delega en `is_subtype`, que recorre la cadena de padres. `lca` calcula el ancestro común más bajo probando primero conformidad en ambos sentidos y, si no, intersecando las listas de ancestros. El entorno registra además los *builtins*: `print` (variádico, parámetro `Unknown`), `sqrt`, `sin`, `cos`, `exp`, `log`, `rand`, `range`, y las constantes `PI` y `E`. Toda esta información (`Environment`) se **traspasa al `SemanticChecker`** tras la inferencia, de modo que el chequeador no recalcula firmas ni jerarquías: las lee del entorno que pobló el inferidor.

---

## 9. El chequeo semántico

`SemanticChecker` (`semantic.rs`) consume el AST anotado y **lee** los tipos en lugar de inferirlos (vía `type_of`, que devuelve el `return_type` ya anotado de cada nodo). Verifica, entre otras reglas: operandos correctos para aritmética (`Number`), comparación, lógica (`Boolean`) y concatenación (con `expect_concatenable`, que admite los tipos coercibles a texto); condiciones de `if`/`elif`/`while` de tipo `Boolean`; variables declaradas antes de usarse en `:=` y conformidad del valor asignado al tipo de la variable; aridad y tipos de argumentos en llamadas a función, constructores y métodos; existencia de campos y métodos en la jerarquía (subiendo por la cadena de herencia); validez de `self` (solo dentro de un método) y de `base()` (solo dentro de un método de un tipo cuyo padre defina ese mismo método); que el iterador de un `for` sea una llamada a `range(...)`; y que los índices de tupla estén en rango.

Los errores se acumulan (no se aborta al primero) con `err`/`err_at`, donde `err_at` lleva el *offset* de byte para que `main` lo traduzca a línea/columna al imprimir. Esto permite reportar varios errores semánticos en una sola pasada en lugar de obligar al usuario a recompilar tras corregir cada uno.

La detección de herencia circular se hace aparte, en `detect_inheritance_cycles`, que recorre la cadena de ancestros de cada tipo declarado y reporta si vuelve sobre un tipo ya visto. Se ejecuta antes de la inferencia precisamente porque la inferencia y el chequeo asumen una jerarquía acíclica.

---

## 10. Generación de código (LLVM IR)

El *backend* (`codegen.rs` + `codegen_visitor.rs`) emite **LLVM IR** textual con *opaque pointers* (el modelo moderno de LLVM, donde `ptr` no lleva tipo apuntado), para el *triple* `x86_64-pc-linux-gnu`. `codegen.rs` contiene la maquinaria del generador (gestión de registros temporales, etiquetas, *scopes* de variables, *layout* de *structs*, construcción de *vtables*, *builtins* y emisión de constructores/métodos), y `codegen_visitor.rs` contiene la implementación del `ExprVisitor` que recorre las expresiones.

### Representación de valores

Los primitivos se mapean directamente: `Number` → `double`, `Boolean` → `i1`, `String` → `ptr` (cadenas estilo C terminadas en nulo). Las clases y `Unknown` también son `ptr`. Cada visita de expresión devuelve un `GeneratorResult`, que empareja el **registro LLVM** donde quedó el valor con su **tipo LLVM**; ese par es lo que se propaga hacia arriba en el recorrido.

### Objetos, herencia y el *layout* de *structs*

Cada clase es un *struct* LLVM con el **puntero a *vtable* en la posición 0**, seguido de los campos en orden **padre-primero**: `collect_all_fields` recolecta recursivamente los campos del padre y luego los propios, de modo que un objeto hijo es un prefijo binario compatible con su padre (un puntero a hijo puede tratarse como puntero a padre sin reajuste). `register_struct_layout` registra ese *layout* y `get_field_index` localiza el índice de un campo dentro del *struct* completo.

El constructor `@T_new`:
1. Calcula el tamaño del objeto con el clásico **GEP-null trick**: `getelementptr %T, ptr null, i32 1` da la dirección del "elemento 1" de un arreglo hipotético que empieza en la dirección nula, que numéricamente es el tamaño del *struct*; `ptrtoint` lo convierte a `i64`. Es la forma portable de obtener `sizeof` en IR sin codificar el tamaño a mano.
2. Reserva memoria con `malloc`.
3. Instala la *vtable*: GEP al campo 0 y `store @vtable_T`.
4. Encadena la inicialización del padre. Si el hijo pasa argumentos explícitos al padre (`inherits Padre(args)`), los evalúa y llama a `@Padre_init_fields` con ellos; si no los pasa, **propaga implícitamente** sus propios parámetros al padre. La función auxiliar `@T_init_fields` permite inicializar campos a lo largo de la jerarquía sin reservar memoria de nuevo (recibe el `self` ya reservado).
5. Inicializa los campos propios calculando su índice real en el *struct* (`1 + número_de_campos_del_padre + posición_del_campo`).

### Despacho dinámico

Las *vtables* se construyen con `build_vtable_for_class`, que parte de una **copia de la *vtable* del padre** (preservando los índices de los *slots* heredados), **sobrescribe** los *slots* de los métodos redefinidos (manteniendo su índice, para que el despacho siga funcionando a través de un puntero a la clase base) y **añade al final** los métodos nuevos. Esta disciplina de "heredados con su índice, redefinidos en su sitio, nuevos al final" es lo que hace correcto el despacho polimórfico. La *vtable* se materializa como un *global* `@vtable_T` con un puntero de función por *slot*. Una llamada a método carga la *vtable* del objeto (campo 0), hace GEP del *slot* correspondiente, carga el puntero de función y lo llama.

### Control de flujo y expresiones que devuelven valor

Como `if`, `while` y `for` son expresiones, su *codegen* tiene que producir un valor. El `if` emite bloques `then`/`elif`/`else`/`merge` con saltos condicionales `br i1`, y en el bloque `merge` usa una instrucción **`phi`** para seleccionar el valor de la rama que efectivamente se ejecutó: la `phi` recibe pares `(valor, bloque_de_procedencia)` y resuelve, en *runtime*, cuál tomar según de dónde venga el control. Es la forma idiomática de SSA (Static Single Assignment) de "el valor de un condicional". El `while` y el `for` acumulan su resultado en un `alloca` (memoria local) que se actualiza en cada iteración, ya que su valor es el de la última iteración del cuerpo.

El `for` merece una nota: como el iterador debe ser una llamada a `range(a, b)`, el generador la **reconoce sintácticamente** y la baja a un bucle de contador explícito. Reserva un contador inicializado a `a`, compara con `b` mediante `fcmp olt double` en el bloque de condición, expone la variable de iteración con el valor actual del contador dentro del cuerpo, y al final de cada vuelta incrementa el contador con `fadd ... 1.0`. No hay un objeto iterador ni un protocolo de iteración general: `range` es un constructo especial que el *codegen* expande en sitio.

### Cadenas, concatenación e igualdad

La concatenación (`@` simple, `@@` con espacio) se implementa con las funciones de `libc`: mide longitudes con `strlen`, reserva el *buffer* resultante con `malloc`, copia con `strcpy` y `strcat` (y, en el caso de `@@`, intercala un espacio). Los operandos que no son cadenas se coercen a texto: `ensure_cstr` convierte un `double` a su representación textual con `snprintf` usando el formato `%g`, y los booleanos se convierten con `select`. La igualdad (`emit_equality`) despacha según el tipo LLVM: `strcmp` para punteros (cadenas), `icmp` para `i1` (booleanos) y `fcmp` para `double` (números).

### *Builtins* como emisores de IR

`print`, `sqrt`, `sin`, `cos`, `exp`, `log` y `rand` no son funciones de biblioteca precompiladas, sino **cierres de Rust** almacenados en un mapa (`builtins: HashMap<String, BuiltinFn>`) que emiten directamente las instrucciones LLVM apropiadas cuando se les invoca. `print` elige el formato de `printf` según el tipo del argumento (`%g` con salto de línea para números, `%s` para cadenas) y devuelve su propio argumento (de modo que `print(x)` tiene el valor de `x`). Las funciones matemáticas emiten llamadas a las correspondientes de `libm` (`@llvm.sqrt`/`sqrt`, `sin`, `cos`, `exp`, `log`). Modelarlas como emisores en lugar de como funciones predefinidas en el IR mantiene el *runtime* mínimo: solo se emite el código de los *builtins* que el programa realmente usa.

### Ensamblado final y enlazado

`compile_hulk_program` emite primero las **declaraciones** de las funciones externas de `libc`/`libm` (`malloc`, `printf`, `snprintf`, `strlen`, `strcpy`, `strcat`, `strcmp`, etc.), las constantes de formato, y la función de error de *cast*. Luego, en una primera pasada, emite los tipos (sus *structs*, *vtables*, constructores y métodos); en una segunda, las funciones libres; y finalmente `@main`, que ejecuta las expresiones de nivel superior. El IR completo se escribe a `output.ll`.

El enlazado lo hace `build_output` en `main.rs`: intenta primero `clang output.ll -o output -lm -O2`, que acepta el `.ll` directamente, enlaza `libm` y optimiza. Si `clang` no está disponible, cae a la ruta alternativa `llc -filetype=obj` (para producir un objeto) seguido de `cc -lm` (para enlazar). El código de salida del compilador refleja el éxito (0) o la fase que falló.

---

## 11. Limitaciones notables

A continuación se listan las limitaciones conocidas de la implementación, útiles para anticipar comportamientos y guiar trabajo futuro:

- **Genéricos solo por instanciación explícita.** Los argumentos de tipo deben darse con *turbofish* (`::<T>`); no se infieren desde el uso. La monomorfización puede provocar *code bloat*, y no existen genéricos acotados/restringidos: en la conformidad, un `Param` conforma con cualquier cosa, por lo que no se verifican restricciones sobre `T`.
- **Inferencia "permisiva".** El inferidor delega *todos* los conflictos de tipo al *checker*; las restricciones `Conform` con variables sin resolver se descartan en silencio, y un condicional cuyas ramas no tienen ancestro común queda como `Unknown`.
- **Precisión numérica.** Los literales numéricos se parsean como `f32` en el lexer, pero el IR usa `double`. Aunque funcional, esto puede introducir pérdidas de precisión respecto a un `double` puro.
- **`for` limitado a `range`.** El iterador de un `for` debe ser exactamente una llamada a `range(a, b)`; no hay iterables generales ni protocolo de iteración.
- **`is`/`as` solo para clases.** No operan sobre primitivos ni tuplas, y `is` es lineal en el número de subtipos del tipo destino (se compara contra la vtable de cada subtipo). El mecanismo depende de que cada clase tenga una vtable única.
- **`print` con tipado laxo.** Se modela como variádico de un parámetro `Unknown`, de modo que su verificación de tipos es deliberadamente débil.
- **Tuplas por valor y sin igualdad.** Las tuplas se cargan como valor de *struct*; no hay una operación de igualdad definida sobre ellas.
- **Plataforma única.** El *triple* objetivo es Linux x86-64 (la variante de Windows está comentada en el código).
- **Sin funciones de primera clase ni clausuras.** El lenguaje no soporta pasar funciones como valores ni capturar entornos.
- **Verbosidad de la gramática.** La duplicación `OpenS`/`OpenB` para resolver la ambigüedad de cuerpos abiertos tiene un coste de mantenimiento: cualquier cambio en la cadena de precedencia debe replicarse en ambas variantes.

---

## 12. Resumen

El proyecto es un compilador de HULK a código nativo, estructurado en fases claramente separadas y con responsabilidades bien repartidas. Un lexer propio basado en regex (con *maximal munch* por orden de reglas, promoción de keywords y errores léxicos precisos) alimenta a un parser LALR(1) generado con LALRPOP, cuya gramática codifica la precedencia como una cascada y resuelve la ambigüedad de cuerpos abiertos triplicando esa cascada en variantes `OpenS`/`OpenB`. El AST resultante se recorre con un único patrón Visitor que **comparten** la inferencia y el *codegen*, lo que evita duplicar la lógica de recorrido. Los genéricos se resuelven por monomorfización en dos pasos (promoción de parámetros y especialización con *worklist* hasta punto fijo, con *mangling* de nombres). Las tuplas tienen nodos y representación LLVM dedicados. `is`/`as` se resuelven por identidad de *vtable*, reutilizando como RTTI la estructura que ya existe para el despacho dinámico.

El sistema de tipos descansa sobre dos módulos con responsabilidades disjuntas: una inferencia por restricciones (con unificación union-find, LCA para condicionales y detección de estancamiento) que **anota** el AST sin emitir errores, y un *checker* separado que **verifica** todas las reglas del lenguaje sobre el AST ya anotado. El *backend* LLVM produce objetos con *vtables*, despacho dinámico, herencia con *layout* padre-primero, control de flujo con `phi`, cadenas vía `libc`, *builtins* como emisores de IR, y finalmente enlaza contra `libc`/`libm` para producir un ejecutable nativo.

En conjunto, las decisiones de diseño priorizan la **separación de responsabilidades** (lexer/parser desacoplados, inferir vs. verificar, maquinaria vs. recorrido en el *codegen*) y la **simplicidad del código generado** (monomorfización sin coste en *runtime*, RTTI gratuito sobre *vtables*), asumiendo a cambio las limitaciones descritas.