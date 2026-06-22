# REPORT — Compilador de HULK

Este documento describe la arquitectura interna del compilador de **HULK** implementado en Rust, las decisiones de diseño que lo sustentan, las características del lenguaje que soporta y sus limitaciones conocidas. El objetivo es que sirva como referencia completa del proyecto: cualquier persona que lo lea debería entender *qué* hace cada componente, *por qué* está hecho así y *dónde* están los límites de la implementación.

El compilador es un **compilador completo a código nativo**: toma un archivo fuente `.hulk`, lo analiza (léxico, sintáctico y semántico), genera **LLVM IR** (`output.ll`) y finalmente produce un ejecutable nativo (`./output`) para Linux x86-64 enlazando contra `libc`/`libm` mediante `clang` (o `llc` + `cc` como alternativa).

---

## 1. Visión general del *pipeline*

El punto de entrada (`main.rs`) orquesta una tubería de fases bien delimitadas. El orden importa, porque cada fase asume invariantes establecidas por la anterior:

1. **Lectura del fuente.** Se lee el archivo pasado como argumento. Un fallo aquí se reporta como error léxico.
2. **Análisis léxico + sintáctico.** Un *lexer* propio (basado en expresiones regulares) alimenta a un parser **LALR(1)** generado con **LALRPOP**. El resultado es un AST (`Program`). Un único error de esta etapa se reporta clasificándolo como LÉXICO o SINTÁCTICO.
3. **Genéricos.** Se ejecuta `promote::promote_program` y después el `Monomorphizer`. Esta fase reescribe el AST: elimina las plantillas genéricas y las sustituye por instancias concretas (monomorfización).
4. **Detección de herencia circular.** `semantic::detect_inheritance_cycles` recorre la cadena de ancestros de cada tipo y detecta ciclos antes de continuar.
5. **Inferencia de tipos.** `TypeInferrer::infer_program` recorre el AST, genera restricciones, las resuelve por unificación y **anota** cada nodo con su `HulkType` concreto.
6. **Chequeo semántico.** `SemanticChecker::check_program` recibe el AST ya anotado y verifica todas las reglas del lenguaje (conformidad de tipos, aridad, existencia de miembros, validez de `self`/`base`, etc.).
7. **Generación de código.** `codegen::compile_hulk_program` emite el LLVM IR a `output.ll`.
8. **Enlazado.** `build_output` invoca a `clang` para producir `./output`; si no está disponible, recurre a `llc` + `cc`.

Cada fase se aborta limpiamente ante errores acumulados, y el código de salida del proceso codifica la fase que falló.

### Contrato de diagnósticos y códigos de salida

El módulo `errors.rs` implementa el contrato de la interfaz del compilador. Los mensajes tienen el formato exacto `(line,col) TYPE: message` con línea y columna **1-based** (y `(0,0)` cuando no hay posición disponible). Los códigos de salida son: **1 = LEXICAL**, **2 = SYNTACTIC**, **3 = SEMANTIC** y **0 = éxito**. La función `from_parse_error` traduce los errores de LALRPOP al contrato, distinguiendo los errores del lexer externo (que llegan envueltos como `ParseError::User` → LÉXICO) del resto de errores de parseo (→ SINTÁCTICO). La función `line_col` convierte un *offset* de byte a coordenadas de línea/columna recorriendo el fuente.

---

## 2. Análisis léxico

El lexer (`lexer/lexer.rs`) es **escrito a mano** sobre expresiones regulares ancladas (`^`), y produce tripletas `(inicio, Token, fin)` que LALRPOP consume como un lexer externo. Las decisiones de diseño relevantes son:

- **Maximal munch determinista por orden de reglas.** Los patrones de varios caracteres se prueban *antes* que los de un carácter, de modo que `@@` gana a `@`, `:=` a `:`, `==` a `=`, y los flotantes (`[0-9]+\.[0-9]+`) a los enteros. El orden de la lista *es* la prioridad.
- **Promoción de palabras clave.** Primero se reconoce un identificador genérico y luego `keyword()` decide si ese texto es realmente una palabra reservada (`if`, `let`, `type`, `function`, `inherits`, `new`, `base`, `self`, `is`, `as`, `Number`, `String`, `Boolean`, …). Esto evita reglas separadas por keyword y mantiene el lexer compacto.
- **Comentarios.** Se soportan comentarios de línea (`// …`) y de bloque (`/* … */`). Un comentario de bloque sin cerrar produce un error léxico explícito.
- **Cadenas con escapes.** Las cadenas reconocen secuencias de escape (`\n`, `\t`, `\r`, `\\`, `\"`, `\'`, `\0`) mediante `unescape_string`; un escape no reconocido produce un error léxico ubicado en el carácter ofensor.
- **Posiciones precisas.** Cada token lleva su rango de bytes `(inicio, fin)`, lo que permite a las fases posteriores ubicar errores con exactitud.

Los `Token` (en `lexer/token.rs`) cubren literales con *payload* (`Int`, `Float`, `Str`, `Ident`), las palabras clave y todos los operadores/puntuación del lenguaje.

---

## 3. Análisis sintáctico

La gramática vive en `grammar.lalrpop` y se compila con **LALRPOP** a un parser LALR(1). Usa el bloque `extern` para conectarse al lexer propio, de modo que el lexado y el parseo están **desacoplados** (esto da control fino sobre comentarios, escapes y *maximal munch* que el lexer interno de LALRPOP no ofrecería con la misma comodidad).

### Precedencia de operadores

La precedencia se codifica como una **cascada de reglas** (cada nivel delega al siguiente, de menor a mayor prioridad):

```
Or (| ||) → And (& &&) → Assign (:=) → Comp (== != >= <= > <)
   → Conc (@ @@) → Is (is) → As (as) → Arith (+ -) → FactAnd (* / %)
   → Unary (+ - !) → Pow (^) → Term
```

`Term` es el nivel atómico: literales numéricos/booleanos/de cadena, `new T(...)`, accesos con punto (`x.campo`, `x.metodo(...)`, `x.0` para tuplas), llamadas a función, construcción de tuplas `(a, b, ...)`, expresiones entre paréntesis, `base(...)`, `self` e identificadores.

### El "problema del cuerpo abierto" (OpenS / OpenB)

HULK es un lenguaje **orientado a expresiones**: `if`, `while`, `for` y `let … in` son expresiones que devuelven un valor, y su cuerpo puede ser tanto una expresión simple como un bloque `{ … }`. Esto introduce ambigüedades análogas al clásico *dangling else*. La gramática las resuelve **duplicando** las cadenas de precedencia en dos variantes paralelas, `…OpenS` (cuerpo "simple") y `…OpenB` (cuerpo de "bloque"), de modo que el parser sepa de forma determinista dónde termina cada cuerpo de control. Es verboso, pero elimina conflictos de la tabla LALR sin recurrir a precedencias artificiales.

### Anotaciones de tipo y genéricos en la sintaxis

La regla `Type` reconoce los tipos primitivos (`Number`, `String`, `Boolean`), clases (`Ident`), tipos genéricos aplicados (`Id<T, U>` → `HulkType::Generic`) y tuplas (`(T1, T2, …)` → `HulkType::Tuple`). Los parámetros genéricos de declaración se capturan con `GenericParams` (`<T, U>`) y la instanciación explícita usa la sintaxis *turbofish* `::<...>` (`TurboArgs`), tanto en llamadas a función como en `new`.

---

## 4. El AST y el patrón Visitor

El núcleo del AST está en `nodes/expr_node.rs`. La enumeración `Expr` representa **todas** las formas de expresión (literales, binarias/unarias, `let`, `if`, `while`, `for`, llamadas, instanciación, acceso a miembros y métodos, `self`, `base`, *downcast*, *type test*, tuplas y acceso a tupla). Cada nodo concreto vive en su propio archivo dentro de `nodes/` y guarda un campo `return_type: HulkType` que se rellena durante la inferencia.

El sistema de tipos del lenguaje se modela con `HulkType`:

```
Number | Bool | String | Class(nombre) | Tuple(Vec<HulkType>)
       | Param(nombre) | Generic(nombre, args) | Unknown
```

`Param` representa un parámetro genérico libre (`T`), `Generic` un tipo genérico aplicado pendiente de monomorfizar, y `Unknown` un tipo aún no inferido. `HulkType` ofrece operaciones clave para los genéricos: `promote_params` (convierte `Class(T)` en `Param(T)`), `subst` (sustituye `Param` por tipos concretos según un mapa), `mangle` (produce un nombre plano apto para identificadores LLVM), `collapse_generic` (colapsa un `Generic` ya concreto a `Class("nombre_manglado")`) y `contains_param`.

El recorrido del AST se hace con el patrón **Visitor**: el *trait* `ExprVisitor<T>` (en `expr_visitor.rs`) define un método `visit_*` por cada forma de expresión, y `Expr::accept` despacha al método correspondiente. Lo elegante de este diseño es que **tanto el inferidor de tipos como el generador de código son visitantes**: `TypeInferrer` implementa `ExprVisitor<InferType>` y `CodeGenerator` implementa `ExprVisitor<GeneratorResult>`. Una sola estructura de recorrido sirve a dos fases muy distintas.

Las posiciones (*spans*) se llevan en los literales (provienen del lexer) y, para los nodos compuestos, se derivan estructuralmente con `Expr::span` combinando los *spans* de las hojas. Cuando no hay posición disponible se reporta `(0,0)`.

---

## 5. Característica adicional: Genéricos (monomorfización)

Los genéricos se implementan por **monomorfización** (al estilo de las plantillas de C++ o los genéricos de Rust): por cada combinación concreta de argumentos de tipo se genera una copia especializada del código. Esto ocurre en dos pasos:

**Paso 1 — Promoción (`generics/promote.rs`).** Dentro de una declaración genérica, los nombres de tipo que coinciden con un parámetro declarado se reescriben de `Class(T)` a `Param(T)`. Esto distingue un parámetro genérico libre de una clase real con ese nombre. La promoción es recursiva sobre tuplas y tipos genéricos aplicados, y cubre parámetros de constructor, atributos, firmas de método, anotaciones de `let` y argumentos *turbofish*.

**Paso 2 — Monomorfización (`generics/mono.rs`).** El `Monomorphizer`:

1. Separa las **plantillas** (funciones y tipos con `generics` no vacío) del resto del programa ("raíces" concretas).
2. Recorre las raíces buscando usos de plantillas con argumentos de tipo concretos (vía `scan_expr`). Cada uso siembra una *worklist* con la instancia solicitada y reescribe el nombre del nodo a su forma **manglada** (p. ej. `Box__Number`), vaciando los `type_args`.
3. Procesa la *worklist* hasta un **punto fijo**: por cada instancia pendiente, especializa la plantilla sustituyendo `Param` por tipos concretos (`specialize_fn` / `specialize_type`), vuelve a escanear el cuerpo especializado para descubrir genéricos anidados, y acumula las instancias.
4. Reensambla el programa colocando primero los tipos concretos, luego las funciones, y después el resto (orden que espera el generador de código).

El *mangling* (`mangle_name`) produce nombres como `base__arg1_arg2`, y las tuplas se codifican como `TupN_…`. Una instancia que aún contiene `Param` (porque está dentro de otra plantilla sin resolver) se pospone: la resolverá la especialización del padre.

**Decisión de diseño:** la monomorfización da generación de código sencilla y sin coste en tiempo de ejecución, a cambio de posible *code bloat* y de exigir **instanciación explícita por *turbofish*** (no se infieren los argumentos de tipo desde el uso).

---

## 6. Característica adicional: Tuplas

Las tuplas tienen nodos propios (`nodes/tuple_node.rs`): `TupleNode` para la construcción `(e1, e2, …)` y `TupleAccessNode` para el acceso por índice `expr.N`. La justificación de no reutilizar `BlockNode` ni `FunCallNode` es semántica: una tupla es un **producto de valores heterogéneos** con índice numérico de acceso, distinto de una secuencia o de una llamada.

- **Tipo.** Se modelan como `HulkType::Tuple(Vec<HulkType>)`. El índice de acceso es un `usize` conocido en tiempo de parseo (`p.0`, `p.1`), lo que simplifica tanto la verificación (`index < len`) como la generación de código (un GEP directo).
- **Inferencia.** El acceso a tupla genera una restricción especial `TupleProject(tipo_tupla, índice, resultado)` que, al resolverse la tupla a un tipo concreto, fija el tipo del elemento proyectado.
- **Código.** En LLVM se representan como *structs* anónimos con nombre derivado de sus elementos (`%Tuple_double_ptr_…`), emitidos bajo demanda con `ensure_tuple_type_emitted`. La construcción reserva el *struct*, almacena cada elemento por GEP y carga el valor; el acceso almacena el valor en un temporal, hace GEP del índice y carga.
- **Verificación.** El *checker* valida que el índice esté dentro de rango y que el acceso se haga sobre un tipo tupla.

---

## 7. Características adicionales: `is` y `as`

`is` (*type test*) y `as` (*downcast*) son operadores de **identificación y conversión de tipos en tiempo de ejecución**, con un mecanismo de RTTI muy económico.

**Semántica (`is`).** `expr is T` devuelve `Boolean`. El *checker* exige que `expr` sea de tipo clase (o `Unknown`, para permitir programas parcialmente inferidos).

**Semántica (`as`).** `expr as T` tiene como tipo estático la clase destino `T`. El *checker* aplica tres reglas: (1) la fuente debe ser de tipo clase; (2) el tipo destino debe estar declarado; (3) fuente y destino deben estar **relacionados** por herencia (uno debe ser ancestro del otro). Un *downcast* entre tipos no relacionados es un error semántico.

**Implementación en tiempo de ejecución (RTTI por identidad de vtable).** La clave está en que **cada clase concreta tiene una *vtable* única** (`@vtable_T`). Para `x is T`, el generador carga el puntero a *vtable* del objeto (campo 0 del *struct*), recolecta todos los subtipos de `T` (`collect_subtypes` recorre `class_meta`) y compara la *vtable* del objeto contra `@vtable_S` de cada subtipo `S`, combinando las comparaciones con `or`. El resultado es un `i1`. Para `x as T` se hace la misma comprobación y se ramifica: si conforma, se devuelve el mismo puntero con el tipo estático destino; si no, se llama a `@hulk_cast_error()`, que imprime un mensaje de error de *runtime* y aborta (`abort` + `unreachable`).

Este enfoque no necesita etiquetas de tipo separadas ni cabeceras de objeto adicionales: reutiliza la *vtable* que ya existe para el despacho dinámico.

---

## 8. Característica adicional: Inferencia de tipos

La inferencia (`type_inferrer.rs`) sigue un enfoque **basado en restricciones** con sabor Hindley-Milner, organizado en cuatro etapas:

1. **Registro de declaraciones.** Se construyen las firmas estáticas de tipos (`TypeInfo`: campos, métodos, parámetros, padre) y funciones. Las posiciones sin anotar reciben **variables de tipo frescas** (`InferType::Var(id)`).
2. **Generación de restricciones.** Un recorrido *bottom-up* del AST (vía `ExprVisitor`) produce restricciones de tres clases: `Eq` (igualdad/unificación exacta), `Conform` (subtipado: el lado izquierdo debe conformar al derecho) y `TupleProject` (proyección de elemento de tupla).
3. **Resolución iterativa.** Un *solver* con *worklist* aplica unificación sobre una **sustitución de union-find plano** (`Substitution`) hasta alcanzar un punto fijo. Detecta estancamiento (*stall*) para no ciclar indefinidamente; las restricciones `Conform` con variables sin resolver se descartan silenciosamente.
4. **Anotación del AST.** Cada `Var` se reemplaza por su tipo concreto y se fija el `return_type` de cada nodo. Para `if`/`elif`/`else` se calcula el **ancestro común más bajo (LCA)** de las ramas, de modo que el tipo de un condicional sea el supertipo común.

**Decisión de diseño central:** el inferidor **no emite errores semánticos**. Solo registra errores estructurales irrecuperables (p. ej. cuando no puede continuar). Todos los conflictos de tipo reales (un `Number` donde se esperaba `String`, una conformidad que falla) se dejan pasar deliberadamente para que el `SemanticChecker` los detecte sobre el AST ya anotado. Esto separa con limpieza dos responsabilidades: *inferir* vs. *verificar*.

El entorno (`Environment`) implementa la relación de **conformidad nominal** (`conforms_concrete`/`is_subtype`) recorriendo la cadena de herencia, el cálculo de LCA y ancestros, y la búsqueda de campos/métodos a través de la jerarquía. Registra además los *builtins*: `print` (variádico, parámetro `Unknown`), `sqrt`, `sin`, `cos`, `exp`, `log`, `rand`, `range`, y las constantes `PI` y `E`.

---

## 9. El chequeo semántico

`SemanticChecker` (`semantic.rs`) consume el AST anotado y lee los tipos en lugar de inferirlos. Verifica, entre otras reglas: operandos correctos para aritmética, comparación, lógica y concatenación; condiciones de `if`/`elif`/`while` de tipo `Boolean`; variables declaradas antes de `:=` y conformidad del valor asignado; aridad y tipos de argumentos en llamadas a función, constructores y métodos; existencia de campos y métodos en la jerarquía; validez de `self` (solo dentro de un método) y de `base()` (solo dentro de un método de un tipo con padre que defina ese mismo método); que el iterador de un `for` sea una llamada a `range(...)`; y que los índices de tupla estén en rango. La detección de herencia circular se hace aparte (`detect_inheritance_cycles`) recorriendo la cadena de ancestros de cada tipo declarado.

---

## 10. Generación de código (LLVM IR)

El *backend* (`codegen.rs` + `codegen_visitor.rs`) emite **LLVM IR** con *opaque pointers*, para el *triple* `x86_64-pc-linux-gnu`. Decisiones y mecanismos clave:

- **Representación de primitivos.** `Number` → `double`, `Boolean` → `i1`, `String` → `ptr` (cadenas estilo C), clases y `Unknown` → `ptr`.
- **Objetos y herencia.** Cada clase es un *struct* con el **puntero a vtable en la posición 0** seguido de los campos en orden *padre-primero* (`collect_all_fields`). El constructor `@T_new` calcula el tamaño con el clásico *GEP-null trick*, reserva con `malloc`, instala la vtable, encadena la inicialización del padre y rellena los campos propios. Una función auxiliar `@T_init_fields` permite encadenar la inicialización a lo largo de la jerarquía, con paso de argumentos al padre explícito o implícito.
- **Despacho dinámico.** Las *vtables* (`build_vtable_for_class`) heredan los *slots* del padre conservando sus índices, sobrescriben los métodos redefinidos y añaden los nuevos al final. Una llamada a método carga la vtable del objeto, hace GEP del *slot* y llama al puntero de función.
- ***Builtins* como emisores de IR.** `print`, `sqrt`, `sin`, `cos`, `exp`, `log` y `rand` se implementan como cierres de Rust que emiten las instrucciones LLVM apropiadas (`printf` con el formato según el tipo, llamadas a `libm`, etc.).
- **Cadenas.** La concatenación (`@`, `@@`) usa `strlen`/`malloc`/`strcpy`/`strcat`, con coerción de números y booleanos a texto vía `snprintf`/`select`. La igualdad usa `strcmp` para punteros, `icmp` para `i1` y `fcmp` para `double`.
- **Ensamblado final.** `@main` ejecuta las expresiones de nivel superior tras emitir tipos y funciones; el IR completo se escribe a `output.ll`.

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

El proyecto es un compilador de HULK a código nativo, estructurado en fases claramente separadas y con responsabilidades bien repartidas: un lexer propio que alimenta a un parser LALRPOP; un AST recorrido por un patrón Visitor compartido entre inferencia y *codegen*; genéricos resueltos por monomorfización con *mangling*; tuplas con nodos y representación LLVM dedicados; `is`/`as` resueltos por identidad de *vtable*; una inferencia por restricciones que **anota** y un *checker* separado que **verifica**; y un *backend* LLVM que produce objetos con *vtables*, despacho dinámico, herencia y enlazado a `libc`/`libm`. Las decisiones de diseño priorizan la separación de responsabilidades y la simplicidad del código generado, asumiendo a cambio las limitaciones descritas (instanciación genérica explícita, ausencia de recolección de basura, iteración restringida a `range`, entre otras).