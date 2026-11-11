//! Built-in tutorials: short guided tours that explain the editor step by step and point
//! at the part of the interface each step talks about.

use crate::app::Tab;

/// The part of the interface a step refers to (it gets highlighted).
#[derive(Clone, Copy, PartialEq)]
pub enum Focus {
    None,
    Tab(Tab),
    MenuBar,
    Toolbar,
}

pub struct Step {
    pub title: String,
    pub body: String,
    pub focus: Focus,
}

pub struct Tutorial {
    pub id: &'static str,
    pub title: String,
    pub summary: String,
    pub steps: Vec<Step>,
}

fn step(title: String, body: String, focus: Focus) -> Step {
    Step { title, body, focus }
}

/// All tutorials, in the current interface language.
pub fn all() -> Vec<Tutorial> {
    vec![
        Tutorial {
            id: "welcome",
            title: tr!("Welcome tour", "Tour de boas-vindas"),
            summary: tr!("A two-minute walk around the editor.", "Um passeio de dois minutos pelo editor."),
            steps: vec![
                step(
                    tr!("Welcome to Cryo Engine", "Bem-vindo à Cryo Engine"),
                    tr!(
                        "This tour shows where everything is. The project that is open is Frostpeak Isles, a small 3D platformer you can play, take apart and change.\n\nUse Next to continue. You can reopen any tutorial from Help > Tutorials (F1).",
                        "Este tour mostra onde fica cada coisa. O projeto aberto é o Frostpeak Isles, um pequeno jogo de plataforma 3D que você pode jogar, desmontar e modificar.\n\nUse Próximo para continuar. Você pode reabrir qualquer tutorial em Ajuda > Tutoriais (F1)."
                    ),
                    Focus::None,
                ),
                step(
                    tr!("Scene: your 3D workspace", "Cena: sua área de trabalho 3D"),
                    tr!(
                        "Click an object to select it; drag the coloured arrows to move it. Hold the right mouse button and use WASD to fly around. The 🎥 Camera menu in the corner lists every way to navigate.",
                        "Clique em um objeto para selecioná-lo; arraste as setas coloridas para movê-lo. Segure o botão direito do mouse e use WASD para voar. O menu 🎥 Câmera no canto lista todas as formas de navegar."
                    ),
                    Focus::Tab(Tab::Scene),
                ),
                step(
                    tr!("Hierarchy: everything in the scene", "Hierarquia: tudo o que está na cena"),
                    tr!(
                        "Every object (entity) is listed here. Drag one onto another to make it a child, so they move together. Right-click for Duplicate and Delete. Names in blue have a script attached.",
                        "Todos os objetos (entidades) aparecem aqui. Arraste um sobre outro para torná-lo filho, e eles se movem juntos. Clique com o botão direito para Duplicar e Excluir. Nomes em azul têm um script."
                    ),
                    Focus::Tab(Tab::Hierarchy),
                ),
                step(
                    tr!("Inspector: what an object is made of", "Inspetor: do que um objeto é feito"),
                    tr!(
                        "Shows the selected object's position, rotation and scale, plus its components: Mesh, Light, Camera, Physics and Cryo Script. Use \"+ Add component\" to give an object new abilities.",
                        "Mostra posição, rotação e escala do objeto selecionado, além dos componentes: Malha, Luz, Câmera, Física e Script Cryo. Use \"+ Adicionar componente\" para dar novas habilidades a um objeto."
                    ),
                    Focus::Tab(Tab::Inspector),
                ),
                step(
                    tr!("Content: your project's files", "Conteúdo: os arquivos do projeto"),
                    tr!(
                        "Scenes, scripts, models, textures and sounds live here. Double-click a script to edit it or a model to place it in the scene. Drag files from your computer onto the window to import them.",
                        "Cenas, scripts, modelos, texturas e sons ficam aqui. Dê um duplo clique em um script para editá-lo ou em um modelo para colocá-lo na cena. Arraste arquivos do computador para a janela para importá-los."
                    ),
                    Focus::Tab(Tab::Content),
                ),
                step(
                    tr!("Console: messages and errors", "Console: mensagens e erros"),
                    tr!(
                        "Anything your scripts print with log(...) shows up here, and so do script errors, always with the line number.",
                        "Tudo o que seus scripts imprimem com log(...) aparece aqui, assim como os erros de script, sempre com o número da linha."
                    ),
                    Focus::Tab(Tab::Console),
                ),
                step(
                    tr!("Play, Pause and Stop", "Jogar, Pausar e Parar"),
                    tr!(
                        "Press ▶ Play (Ctrl+P) to run the game inside the editor and Esc to stop. When you stop, the scene goes back to how it was before, so experimenting is safe.",
                        "Pressione ▶ Jogar (Ctrl+P) para rodar o jogo dentro do editor e Esc para parar. Ao parar, a cena volta a ser como era antes, então pode experimentar à vontade."
                    ),
                    Focus::Toolbar,
                ),
                step(
                    tr!("Try the game", "Experimente o jogo"),
                    tr!(
                        "Press ▶ Play now and look for the five Frost Shards.\n\nWASD moves, Space jumps, F throws the cap, Shift crouches, the arrow keys turn the camera.\n\nNext: \"Moving the camera\" and \"Scripting with Cryo\" in Help > Tutorials.",
                        "Pressione ▶ Jogar agora e procure os cinco Frost Shards.\n\nWASD move, Espaço pula, F lança o boné, Shift agacha, as setas giram a câmera.\n\nDepois: \"Movendo a câmera\" e \"Programando com Cryo\" em Ajuda > Tutoriais."
                    ),
                    Focus::Tab(Tab::Game),
                ),
            ],
        },
        Tutorial {
            id: "camera",
            title: tr!("Moving the camera", "Movendo a câmera"),
            summary: tr!("Fly, orbit, zoom, snap to views and save bookmarks.", "Voar, orbitar, aproximar, vistas alinhadas e marcadores."),
            steps: vec![
                step(
                    tr!("Fly", "Voar"),
                    tr!(
                        "In the Scene tab, hold the right mouse button and move the mouse to look around. While holding it, W A S D move, Q and E go down and up, and Shift goes faster. The mouse wheel changes the flight speed.",
                        "Na aba Cena, segure o botão direito do mouse e mova o mouse para olhar em volta. Enquanto segura, W A S D movem, Q e E descem e sobem, e Shift acelera. A roda do mouse muda a velocidade de voo."
                    ),
                    Focus::Tab(Tab::Scene),
                ),
                step(
                    tr!("Orbit, pan and zoom", "Orbitar, deslocar e aproximar"),
                    tr!(
                        "Alt + drag orbits around the point you are looking at. The middle button pans. The wheel zooms.\n\nOn a trackpad, turn on \"Trackpad gestures\" in the 🎥 Camera menu: two fingers orbit, Shift + two fingers pan, pinch zooms.",
                        "Alt + arrastar orbita em volta do ponto para onde você olha. O botão do meio desloca. A roda aproxima.\n\nEm um trackpad, ligue \"Gestos de trackpad\" no menu 🎥 Câmera: dois dedos orbitam, Shift + dois dedos desloca, pinça aproxima."
                    ),
                    Focus::Tab(Tab::Scene),
                ),
                step(
                    tr!("Frame what matters", "Enquadre o que importa"),
                    tr!(
                        "Select an object and press F to bring it into view (double-clicking it does the same). Home frames the whole scene.",
                        "Selecione um objeto e pressione F para trazê-lo para a vista (um duplo clique nele faz o mesmo). Home enquadra a cena inteira."
                    ),
                    Focus::Tab(Tab::Scene),
                ),
                step(
                    tr!("Aligned views", "Vistas alinhadas"),
                    tr!(
                        "Click the X, Y and Z dots of the orientation gizmo in the top-right corner to look from the Front, Right or Top without perspective, which is great for lining things up. Press O to switch between perspective and orthographic.",
                        "Clique nas bolinhas X, Y e Z do indicador de orientação no canto superior direito para olhar de Frente, da Direita ou do Topo sem perspectiva, ótimo para alinhar objetos. Pressione O para alternar entre perspectiva e ortográfica."
                    ),
                    Focus::Tab(Tab::Scene),
                ),
                step(
                    tr!("Bookmarks and the game camera", "Marcadores e a câmera do jogo"),
                    tr!(
                        "Ctrl+1 to Ctrl+4 save where the camera is; 1 to 4 fly back there.\n\nTo place the game's camera, look at the scene the way you want it and choose \"Align game camera to view\" in the 🎥 Camera menu.",
                        "Ctrl+1 a Ctrl+4 salvam onde a câmera está; 1 a 4 voltam para lá.\n\nPara posicionar a câmera do jogo, olhe para a cena do jeito que quiser e escolha \"Alinhar câmera do jogo à vista\" no menu 🎥 Câmera."
                    ),
                    Focus::Tab(Tab::Scene),
                ),
            ],
        },
        Tutorial {
            id: "scene",
            title: tr!("Building a scene", "Montando uma cena"),
            summary: tr!("Create, move, group and save objects.", "Criar, mover, agrupar e salvar objetos."),
            steps: vec![
                step(
                    tr!("Create objects", "Crie objetos"),
                    tr!(
                        "Use \"+ Create\" in the toolbar (or the Object menu) to add cubes, spheres, lights and cameras. New objects appear in front of the camera.",
                        "Use \"+ Criar\" na barra de ferramentas (ou o menu Objeto) para adicionar cubos, esferas, luzes e câmeras. Os novos objetos aparecem à frente da câmera."
                    ),
                    Focus::Toolbar,
                ),
                step(
                    tr!("Move, rotate and scale", "Mova, gire e escale"),
                    tr!(
                        "Pick a tool: W moves, E rotates, R scales. Then drag one of the coloured handles: red is X, green is Y (up) and blue is Z. For exact values, type them in the Inspector.",
                        "Escolha uma ferramenta: W move, E gira, R escala. Depois arraste uma das alças coloridas: vermelho é X, verde é Y (para cima) e azul é Z. Para valores exatos, digite no Inspetor."
                    ),
                    Focus::Tab(Tab::Scene),
                ),
                step(
                    tr!("Group with parents", "Agrupe com pais e filhos"),
                    tr!(
                        "In the Hierarchy, drag an object onto another to make it a child. Children follow their parent, which is how the hero of the example is built: a root with the collider and a model made of several shapes.",
                        "Na Hierarquia, arraste um objeto sobre outro para torná-lo filho. Os filhos seguem o pai; é assim que o herói do exemplo é montado: uma raiz com o colisor e um modelo feito de várias formas."
                    ),
                    Focus::Tab(Tab::Hierarchy),
                ),
                step(
                    tr!("Duplicate, undo and save", "Duplique, desfaça e salve"),
                    tr!(
                        "Ctrl+D duplicates the selection, Del deletes it, Ctrl+Z undoes and Ctrl+Shift+Z redoes. Ctrl+S saves the scene; an asterisk in the status bar means there are unsaved changes.",
                        "Ctrl+D duplica a seleção, Del exclui, Ctrl+Z desfaz e Ctrl+Shift+Z refaz. Ctrl+S salva a cena; um asterisco na barra de status indica alterações não salvas."
                    ),
                    Focus::MenuBar,
                ),
                step(
                    tr!("Light and sky", "Luz e céu"),
                    tr!(
                        "Click an empty spot of the Scene so nothing is selected: the Inspector then shows the scene settings, with ambient light, sky colours and gravity. The first directional light casts the shadows.",
                        "Clique em um ponto vazio da Cena para não ter nada selecionado: o Inspetor mostra as configurações da cena, com luz ambiente, cores do céu e gravidade. A primeira luz direcional projeta as sombras."
                    ),
                    Focus::Tab(Tab::Inspector),
                ),
            ],
        },
        Tutorial {
            id: "import",
            title: tr!("Importing models, textures and sounds", "Importando modelos, texturas e sons"),
            summary: tr!("Bring your own .obj, .gltf, .glb, images and audio.", "Traga seus .obj, .gltf, .glb, imagens e áudio."),
            steps: vec![
                step(
                    tr!("Drop files on the window", "Solte arquivos na janela"),
                    tr!(
                        "Drag files from your computer onto the editor, or click ⬇ Import in the Content tab. Models (.obj, .gltf, .glb) go to models/, images (.png, .jpg) to textures/ and sounds (.wav, .ogg, .mp3) to sounds/. Materials and textures a model refers to are copied too.",
                        "Arraste arquivos do computador para o editor, ou clique em ⬇ Importar na aba Conteúdo. Modelos (.obj, .gltf, .glb) vão para models/, imagens (.png, .jpg) para textures/ e sons (.wav, .ogg, .mp3) para sounds/. Materiais e texturas que o modelo usa são copiados junto."
                    ),
                    Focus::Tab(Tab::Content),
                ),
                step(
                    tr!("Place a model", "Coloque um modelo"),
                    tr!(
                        "An imported model is placed in front of the camera. Later, double-click it in the Content tab to add another copy. If it was made in other units, its scale is adjusted to about 2 m; \"Fit size\" in the Inspector does that again.",
                        "Um modelo importado entra na cena à frente da câmera. Depois, dê um duplo clique nele na aba Conteúdo para adicionar outra cópia. Se foi feito em outra unidade, a escala é ajustada para cerca de 2 m; \"Ajustar tamanho\" no Inspetor refaz isso."
                    ),
                    Focus::Tab(Tab::Scene),
                ),
                step(
                    tr!("Textures and colours", "Texturas e cores"),
                    tr!(
                        "In the Mesh component choose a Texture, or select an object and double-click an image in the Content tab. The colour multiplies the texture: keep it white to see the image as it is.",
                        "No componente Malha escolha uma Textura, ou selecione um objeto e dê um duplo clique em uma imagem na aba Conteúdo. A cor multiplica a textura: deixe branca para ver a imagem como ela é."
                    ),
                    Focus::Tab(Tab::Inspector),
                ),
                step(
                    tr!("Play sounds from a script", "Toque sons por script"),
                    tr!(
                        "Sounds are played by scripts:\n\naudio.play(\"sounds/coin.wav\")\naudio.music(\"sounds/theme.ogg\")\n\nThe first plays once; the second loops.",
                        "Os sons são tocados por scripts:\n\naudio.play(\"sounds/coin.wav\")\naudio.music(\"sounds/theme.ogg\")\n\nO primeiro toca uma vez; o segundo fica em repetição."
                    ),
                    Focus::Tab(Tab::Script),
                ),
            ],
        },
        Tutorial {
            id: "scripting",
            title: tr!("Scripting with Cryo", "Programando com Cryo"),
            summary: tr!("Give objects behaviour with the Cryo language.", "Dê comportamento aos objetos com a linguagem Cryo."),
            steps: vec![
                step(
                    tr!("Attach a script", "Anexe um script"),
                    tr!(
                        "Select an object, click \"+ Add component\" > Cryo Script, then \"+ New script\". The file is created in scripts/ and opens in the script editor.",
                        "Selecione um objeto, clique em \"+ Adicionar componente\" > Script Cryo e depois em \"+ Novo script\". O arquivo é criado em scripts/ e abre no editor de scripts."
                    ),
                    Focus::Tab(Tab::Inspector),
                ),
                step(
                    tr!("Events", "Eventos"),
                    tr!(
                        "A script reacts to events:\n\non start() {{ }}        once, when the game starts\non update(dt) {{ }}     every frame\non collide(other, normal) {{ }}   on contact\n\nInside them, `self` is the object the script is attached to.",
                        "Um script reage a eventos:\n\non start() {{ }}        uma vez, quando o jogo começa\non update(dt) {{ }}     a cada quadro\non collide(other, normal) {{ }}   ao encostar\n\nDentro deles, `self` é o objeto ao qual o script está anexado."
                    ),
                    Focus::Tab(Tab::Script),
                ),
                step(
                    tr!("Make it move", "Faça-o se mover"),
                    tr!(
                        "Try this inside update:\n\nself.rotate(vec3(0, 90 * dt, 0))\n\nPress Ctrl+S and then ▶ Play: the object spins 90 degrees per second. Multiplying by dt keeps the speed the same on any computer.",
                        "Experimente dentro de update:\n\nself.rotate(vec3(0, 90 * dt, 0))\n\nPressione Ctrl+S e depois ▶ Jogar: o objeto gira 90 graus por segundo. Multiplicar por dt mantém a velocidade igual em qualquer computador."
                    ),
                    Focus::Tab(Tab::Script),
                ),
                step(
                    tr!("Props show up in the Inspector", "Props aparecem no Inspetor"),
                    tr!(
                        "Declare `prop speed: num = 5` at the top of a script and the value becomes editable in the Inspector, per object, with no code changes. Use `var` for internal state.",
                        "Declare `prop speed: num = 5` no início do script e o valor passa a ser editável no Inspetor, por objeto, sem mexer no código. Use `var` para estado interno."
                    ),
                    Focus::Tab(Tab::Inspector),
                ),
                step(
                    tr!("Talk to other objects", "Converse com outros objetos"),
                    tr!(
                        "scene.find(\"Player\") returns another entity. Read or set its position, or send it your own event:\n\nplayer.send(\"coin\")\n\nwhich runs `on coin() {{ }}` in that object's script. Coin.cryo and Player.cryo in the example do exactly this.",
                        "scene.find(\"Player\") devolve outra entidade. Leia ou altere a posição dela, ou envie um evento seu:\n\nplayer.send(\"coin\")\n\nque executa `on coin() {{ }}` no script daquele objeto. Coin.cryo e Player.cryo no exemplo fazem exatamente isso."
                    ),
                    Focus::Tab(Tab::Script),
                ),
                step(
                    tr!("When something goes wrong", "Quando algo dá errado"),
                    tr!(
                        "The script editor checks the code as you type and shows the first error next to the Save button. Errors while the game runs appear in the Console with the line number; only that script stops, the editor keeps going.",
                        "O editor de scripts confere o código enquanto você digita e mostra o primeiro erro ao lado do botão Salvar. Erros com o jogo rodando aparecem no Console com o número da linha; só aquele script para, o editor continua."
                    ),
                    Focus::Tab(Tab::Console),
                ),
            ],
        },
        Tutorial {
            id: "physics",
            title: tr!("Physics and collisions", "Física e colisões"),
            summary: tr!("Gravity, solid objects, triggers and contact events.", "Gravidade, objetos sólidos, gatilhos e eventos de contato."),
            steps: vec![
                step(
                    tr!("Add Physics", "Adicione Física"),
                    tr!(
                        "Add the Physics component to an object. Static bodies never move (floors, walls). Dynamic bodies fall with gravity and are pushed out of whatever they touch.",
                        "Adicione o componente Física a um objeto. Corpos estáticos nunca se movem (chão, paredes). Corpos dinâmicos caem com a gravidade e são empurrados para fora do que encostam."
                    ),
                    Focus::Tab(Tab::Inspector),
                ),
                step(
                    tr!("Colliders", "Colisores"),
                    tr!(
                        "The collider is a box or a sphere sized from the object's scale (for imported models, from the model's bounds). The selected object's box collider is drawn in green in the Scene. Boxes can be rotated, which is how ramps work.",
                        "O colisor é uma caixa ou uma esfera dimensionada pela escala do objeto (em modelos importados, pelos limites do modelo). O colisor de caixa do objeto selecionado aparece em verde na Cena. Caixas podem ser giradas; é assim que rampas funcionam."
                    ),
                    Focus::Tab(Tab::Scene),
                ),
                step(
                    tr!("Triggers", "Gatilhos"),
                    tr!(
                        "Tick Trigger to detect contact without blocking: coins, checkpoints and hint areas in the example are triggers.",
                        "Marque Gatilho para detectar contato sem bloquear: moedas, checkpoints e áreas de dica no exemplo são gatilhos."
                    ),
                    Focus::Tab(Tab::Inspector),
                ),
                step(
                    tr!("React in a script", "Reaja em um script"),
                    tr!(
                        "on collide(other, normal) runs on both objects. `normal` points from `other` towards you: if normal.y is close to 1 you are standing on it, close to -1 it is on top of you.\n\nIn scripts, move dynamic bodies with self.velocity and check self.grounded before jumping.",
                        "on collide(other, normal) roda nos dois objetos. `normal` aponta de `other` para você: se normal.y está perto de 1 você está em cima dele; perto de -1, ele está em cima de você.\n\nNos scripts, mova corpos dinâmicos com self.velocity e confira self.grounded antes de pular."
                    ),
                    Focus::Tab(Tab::Script),
                ),
            ],
        },
        Tutorial {
            id: "animation",
            title: tr!("Animation, cutscenes and timed events", "Animação, cutscenes e eventos sincronizados"),
            summary: tr!("Keyframes on a timeline, with sounds and script events in sync.", "Keyframes em uma linha do tempo, com sons e eventos de script sincronizados."),
            steps: vec![
                step(
                    tr!("The Timeline", "A Linha do tempo"),
                    tr!(
                        "Open the Timeline tab. An animation (clip) moves objects over time and fires events at exact instants. The example has two: Intro, the opening cutscene, and Victory, started by a script.",
                        "Abra a aba Linha do tempo. Uma animação (clipe) move objetos ao longo do tempo e dispara eventos em instantes exatos. O exemplo tem duas: Intro, a cutscene de abertura, e Victory, iniciada por um script."
                    ),
                    Focus::Tab(Tab::Timeline),
                ),
                step(
                    tr!("Set keyframes", "Crie keyframes"),
                    tr!(
                        "Click + New animation. Select an object, then click + Position: that records where it is now. Drag the ruler to another time, move the object in the Scene and click + Position again. Press Preview to watch it travel between the two keys.",
                        "Clique em + Nova animação. Selecione um objeto e clique em + Posição: isso grava onde ele está agora. Arraste a régua para outro tempo, mova o objeto na Cena e clique em + Posição de novo. Pressione Prévia para vê-lo ir de uma chave à outra."
                    ),
                    Focus::Tab(Tab::Timeline),
                ),
                step(
                    tr!("Edit keys", "Edite as chaves"),
                    tr!(
                        "Click a key to change its time, value and what happens after it: Smooth, Linear or Step (hold). Drag a key sideways to retime it. Rotation, Scale, Colour and a camera's Field of view are animated the same way.",
                        "Clique em uma chave para mudar o tempo, o valor e o que acontece depois dela: Suave, Linear ou Degrau (segura). Arraste a chave para os lados para mudar o tempo. Rotação, Escala, Cor e o Campo de visão de uma câmera são animados do mesmo jeito."
                    ),
                    Focus::Tab(Tab::Timeline),
                ),
                step(
                    tr!("Events in sync", "Eventos sincronizados"),
                    tr!(
                        "Move the playhead and choose + Event to make something happen at that exact instant: play a Sound, send a Script event to an object (it runs `on my_event()` there), cut to another Camera, or show Screen text.",
                        "Mova o cursor de tempo e escolha + Evento para algo acontecer naquele instante exato: tocar um Som, enviar um Evento de script a um objeto (executa `on my_event()` nele), cortar para outra Câmera ou mostrar Texto na tela."
                    ),
                    Focus::Tab(Tab::Timeline),
                ),
                step(
                    tr!("Cutscenes", "Cutscenes"),
                    tr!(
                        "Tick Cutscene to block the player's input and show cinema bars while the clip plays (Enter skips it). Tick Autoplay to start it with the game. A typical cutscene animates a second camera and uses Camera cut events to switch to it and back.",
                        "Marque Cutscene para bloquear os comandos do jogador e mostrar faixas de cinema enquanto o clipe toca (Enter pula). Marque Tocar ao iniciar para começar junto com o jogo. Uma cutscene típica anima uma segunda câmera e usa eventos de Corte de câmera para trocar para ela e voltar."
                    ),
                    Focus::Tab(Tab::Timeline),
                ),
                step(
                    tr!("Animations in real time", "Animações em tempo real"),
                    tr!(
                        "Start a clip from any script:\n\nanim.play(\"Victory\")\nanim.stop(\"Victory\")\nanim.playing(\"Victory\")\n\nWhen a clip ends, every script receives `on animation_end(name)`. Doors, lifts and effects are usually done this way.",
                        "Inicie um clipe de qualquer script:\n\nanim.play(\"Victory\")\nanim.stop(\"Victory\")\nanim.playing(\"Victory\")\n\nQuando um clipe termina, todo script recebe `on animation_end(name)`. Portas, elevadores e efeitos costumam ser feitos assim."
                    ),
                    Focus::Tab(Tab::Script),
                ),
                step(
                    tr!("Timers in scripts", "Temporizadores nos scripts"),
                    tr!(
                        "For timing without a timeline:\n\nafter(2, \"explode\")      once, 2 seconds from now\nevery(0.5, \"blink\")      again and again\ncancel(\"blink\")\n\nEach one runs the matching `on` handler of the same script. HintZone.cryo uses after() to re-arm itself.",
                        "Para sincronizar sem linha do tempo:\n\nafter(2, \"explode\")      uma vez, daqui a 2 segundos\nevery(0.5, \"blink\")      repetidamente\ncancel(\"blink\")\n\nCada um executa o `on` correspondente do mesmo script. HintZone.cryo usa after() para se rearmar."
                    ),
                    Focus::Tab(Tab::Script),
                ),
            ],
        },
        Tutorial {
            id: "modelling",
            title: tr!("Modelling in the engine", "Modelando dentro da engine"),
            summary: tr!("Shape your own meshes: extrude, inset, subdivide and more.", "Crie suas próprias malhas: extrudar, inserir, subdividir e mais."),
            steps: vec![
                step(
                    tr!("Enter Edit Mesh", "Entre em Editar malha"),
                    tr!(
                        "Choose + Create > Editable mesh, or select any object with a Mesh and click Edit mesh in the Inspector. A primitive is first saved as a model file in models/, so the original shapes are never changed.",
                        "Escolha + Criar > Malha editável, ou selecione um objeto com Malha e clique em Editar malha no Inspetor. Uma primitiva é salva antes como arquivo de modelo em models/, então as formas originais nunca são alteradas."
                    ),
                    Focus::Tab(Tab::Inspector),
                ),
                step(
                    tr!("Select faces or vertices", "Selecione faces ou vértices"),
                    tr!(
                        "The bar at the top of the Scene switches between Faces and Vertices. Click to select, Shift + click to add or remove, A selects everything. The camera works as usual.",
                        "A barra no topo da Cena alterna entre Faces e Vértices. Clique para selecionar, Shift + clique para adicionar ou remover, A seleciona tudo. A câmera funciona como sempre."
                    ),
                    Focus::Tab(Tab::Scene),
                ),
                step(
                    tr!("Move, rotate, scale", "Mova, gire, escale"),
                    tr!(
                        "The gizmo appears on the selection. Use the Move, Rotate and Scale tools (W, E, R) and drag an axis to reshape the mesh.",
                        "O gizmo aparece sobre a seleção. Use as ferramentas Mover, Girar e Escala (W, E, R) e arraste um eixo para remodelar a malha."
                    ),
                    Focus::Tab(Tab::Scene),
                ),
                step(
                    tr!("Extrude and inset", "Extrudar e inserir"),
                    tr!(
                        "Extrude (X) pulls the selected faces outwards, creating new sides: select the top of a cube, extrude and drag it up to make a tower. Inset (I) creates a smaller face inside each one, the usual start for windows, rims and steps.",
                        "Extrudar (X) puxa as faces selecionadas para fora, criando novas laterais: selecione o topo de um cubo, extrude e arraste para cima para fazer uma torre. Inserir (I) cria uma face menor dentro de cada uma, o começo usual de janelas, bordas e degraus."
                    ),
                    Focus::Tab(Tab::Scene),
                ),
                step(
                    tr!("More tools", "Mais ferramentas"),
                    tr!(
                        "Subdivide (U) splits faces for more detail. Delete removes the selection. In Vertices mode, Merge (M) joins vertices at their centre and Fill builds a face from three or more vertices. Flip (N) turns faces that look dark or see-through. Smooth switches between flat and rounded shading.",
                        "Subdividir (U) divide as faces para ter mais detalhe. Excluir remove a seleção. No modo Vértices, Unir (M) junta vértices no centro e Preencher cria uma face a partir de três ou mais vértices. Inverter (N) vira faces que aparecem escuras ou transparentes. Suave alterna entre sombreamento plano e arredondado."
                    ),
                    Focus::Tab(Tab::Scene),
                ),
                step(
                    tr!("Finish", "Conclua"),
                    tr!(
                        "Ctrl+Z undoes modelling steps. Done saves the model as an .obj in models/ (every object using that file updates); Cancel goes back to how it was. The box collider and textures follow the new shape.",
                        "Ctrl+Z desfaz os passos de modelagem. Concluir salva o modelo como .obj em models/ (todos os objetos que usam o arquivo são atualizados); Cancelar volta ao que era. O colisor de caixa e as texturas acompanham a nova forma."
                    ),
                    Focus::Tab(Tab::Scene),
                ),
            ],
        },
        Tutorial {
            id: "events",
            title: tr!("Events and quick time events", "Eventos e quick time events"),
            summary: tr!("Make scripts talk to each other and ask the player to react in time.", "Faça os scripts conversarem e peça ao jogador para reagir a tempo."),
            steps: vec![
                step(
                    tr!("Events between scripts", "Eventos entre scripts"),
                    tr!(
                        "other.send(\"hit\", 2) runs on hit(amount) in one object. emit(\"alarm\") runs on alarm() in every script of the scene, so a lever can warn all the doors without knowing them. global.score keeps a value that every script can read and change.",
                        "other.send(\"hit\", 2) executa on hit(amount) em um objeto. emit(\"alarm\") executa on alarm() em todos os scripts da cena, então uma alavanca avisa todas as portas sem conhecê-las. global.score guarda um valor que todos os scripts leem e alteram."
                    ),
                    Focus::Tab(Tab::Script),
                ),
                step(
                    tr!("Events the engine sends", "Eventos que a engine envia"),
                    tr!(
                        "Besides start, update and collide, a script can declare on touch_start(other) and on touch_end(other), fired once when a contact begins and ends, and on key_down(key) and on key_up(key). Give objects a Tag in the Inspector and find them all with scene.tagged(\"enemy\").",
                        "Além de start, update e collide, um script pode declarar on touch_start(other) e on touch_end(other), disparados uma vez quando um contato começa e termina, e on key_down(key) e on key_up(key). Dê uma Etiqueta aos objetos no Inspetor e encontre todos com scene.tagged(\"enemy\")."
                    ),
                    Focus::Tab(Tab::Inspector),
                ),
                step(
                    tr!("A quick time event in one line", "Um quick time event em uma linha"),
                    tr!(
                        "qte.press(\"e\", 1.5) shows the key on screen with a ring that runs out in 1.5 seconds. There are also qte.mash(key, presses, seconds), qte.hold(key, hold_seconds, seconds) and qte.sequence([\"w\", \"a\", \"d\"], seconds). The answer arrives as on qte_success(name) or on qte_fail(name). Set time.scale = 0.3 for slow motion while the player decides.",
                        "qte.press(\"e\", 1.5) mostra a tecla na tela com um anel que se esgota em 1,5 segundo. Há também qte.mash(tecla, apertos, segundos), qte.hold(tecla, segundos_segurando, segundos) e qte.sequence([\"w\", \"a\", \"d\"], segundos). A resposta chega como on qte_success(nome) ou on qte_fail(nome). Use time.scale = 0.3 para câmera lenta enquanto o jogador decide."
                    ),
                    Focus::Tab(Tab::Script),
                ),
                step(
                    tr!("Quick time events in cutscenes", "Quick time events em cutscenes"),
                    tr!(
                        "In the Timeline, + Event > Quick time event places one at an exact instant of a clip. The clip waits there until the player answers, then goes on; every script hears the result, so one can start a different animation on failure. The same menu has Event for everyone, Play animation and Game speed.",
                        "Na Timeline, + Evento > Quick time event coloca um em um instante exato do clipe. O clipe espera ali até o jogador responder e então continua; todos os scripts recebem o resultado, então um deles pode iniciar outra animação em caso de falha. O mesmo menu tem Evento para todos, Tocar animação e Velocidade do jogo."
                    ),
                    Focus::Tab(Tab::Timeline),
                ),
                step(
                    tr!("State machines", "Máquinas de estado"),
                    tr!(
                        "Inside a script, state patrol and state chase blocks group their own on enter(), on update(dt) and on exit(). The script is in one state at a time and goto chase switches. It keeps enemy and door logic short and readable.",
                        "Dentro de um script, blocos state patrol e state chase agrupam seus próprios on enter(), on update(dt) e on exit(). O script fica em um estado por vez e goto chase troca. Isso mantém a lógica de inimigos e portas curta e legível."
                    ),
                    Focus::Tab(Tab::Script),
                ),
            ],
        },
        Tutorial {
            id: "firstperson",
            title: tr!("First person and animated models", "Primeira pessoa e modelos animados"),
            summary: tr!("A first-person camera in one click, and models that move with their skeleton.", "Uma câmera em primeira pessoa em um clique, e modelos que se movem com o esqueleto."),
            steps: vec![
                step(
                    tr!("Create the player", "Crie o jogador"),
                    tr!(
                        "Choose + Create > First-person player. It adds a body with physics, a camera called Head at eye height and the FirstPerson script. Press Play: the mouse looks around, WASD walks, Shift runs, Space jumps. Tab frees the mouse and Esc stops the game.",
                        "Escolha + Criar > Jogador em primeira pessoa. Isso adiciona um corpo com física, uma câmera chamada Head na altura dos olhos e o script FirstPerson. Aperte Play: o mouse olha em volta, WASD anda, Shift corre, Espaço pula. Tab solta o mouse e Esc para o jogo."
                    ),
                    Focus::Tab(Tab::Scene),
                ),
                step(
                    tr!("Tune it", "Ajuste"),
                    tr!(
                        "Select the player: speed, jump height, mouse sensitivity and head bob are properties of the script in the Inspector. In your own scripts, input.lock_mouse(true) holds the mouse and input.mouse_dx and input.mouse_dy give its movement.",
                        "Selecione o jogador: velocidade, altura do pulo, sensibilidade do mouse e balanço da cabeça são propriedades do script no Inspetor. Nos seus scripts, input.lock_mouse(true) prende o mouse e input.mouse_dx e input.mouse_dy dão o movimento."
                    ),
                    Focus::Tab(Tab::Inspector),
                ),
                step(
                    tr!("Animated models", "Modelos animados"),
                    tr!(
                        "A glTF model with a skeleton brings its animation clips along. Try + Create > Animated model (example), then pick a clip in Mesh > Animation in the Inspector: the selected model previews it in the Scene. From a script, self.animate(\"Walk\") switches clips with a short blend.",
                        "Um modelo glTF com esqueleto traz seus clipes de animação. Experimente + Criar > Modelo animado (exemplo) e escolha um clipe em Malha > Animação no Inspetor: o modelo selecionado mostra a prévia na Cena. Em um script, self.animate(\"Walk\") troca de clipe com uma transição curta."
                    ),
                    Focus::Tab(Tab::Inspector),
                ),
                step(
                    tr!("Levels as colliders, bodies that tumble", "Fases como colisor, corpos que tombam"),
                    tr!(
                        "In Physics, the Mesh collider makes a static imported model solid triangle by triangle, so the player walks on the real shape of a level. Can rotate lets a dynamic body spin, roll and tip over when hit.",
                        "Em Física, o colisor Malha torna um modelo importado estático sólido triângulo por triângulo, então o jogador anda sobre a forma real da fase. Pode girar deixa um corpo dinâmico girar, rolar e tombar quando atingido."
                    ),
                    Focus::Tab(Tab::Inspector),
                ),
            ],
        },
        Tutorial {
            id: "platformer",
            title: tr!("Inside the example platformer", "Por dentro do jogo de exemplo"),
            summary: tr!("How Frostpeak Isles is put together.", "Como o Frostpeak Isles é montado."),
            steps: vec![
                step(
                    tr!("Controls", "Controles"),
                    tr!(
                        "WASD: move · Space: jump · Shift: crouch · F or click: throw the cap · E: leave a captured creature · arrow keys or right-drag: camera.\n\nThree jumps in a row: triple jump. Shift + Space: backflip, or long jump while running. Shift in the air: ground pound. Jump against a wall and press Space: wall jump.",
                        "WASD: mover · Espaço: pular · Shift: agachar · F ou clique: lançar o boné · E: sair de uma criatura capturada · setas ou arrastar com o botão direito: câmera.\n\nTrês pulos seguidos: pulo triplo. Shift + Espaço: mortal para trás, ou pulo longo correndo. Shift no ar: pisão. Pule contra uma parede e pressione Espaço: pulo na parede."
                    ),
                    Focus::Tab(Tab::Game),
                ),
                step(
                    tr!("The hero", "O herói"),
                    tr!(
                        "Select Player in the Hierarchy. The root has the Physics box and the Player.cryo script and no mesh; its child KipModel holds the body, head and hat and turns to face where you run, so the collider never rotates.",
                        "Selecione Player na Hierarquia. A raiz tem a caixa de Física e o script Player.cryo, sem malha; o filho KipModel contém corpo, cabeça e boné e gira para onde você corre, de modo que o colisor nunca gira."
                    ),
                    Focus::Tab(Tab::Hierarchy),
                ),
                step(
                    tr!("One script per idea", "Um script por ideia"),
                    tr!(
                        "Open the scripts folder: Player, Cap, OrbitCamera, Coin, Shard, Checkpoint, MovingPlatform, BouncePad, Spinner, Spiker, Hopper and HintZone. Each is short and talks to the others with send(...). Start with Coin.cryo, then read Player.cryo.",
                        "Abra a pasta scripts: Player, Cap, OrbitCamera, Coin, Shard, Checkpoint, MovingPlatform, BouncePad, Spinner, Spiker, Hopper e HintZone. Cada um é curto e conversa com os outros via send(...). Comece por Coin.cryo e depois leia Player.cryo."
                    ),
                    Focus::Tab(Tab::Content),
                ),
                step(
                    tr!("Tune it without code", "Ajuste sem código"),
                    tr!(
                        "Select Player and change run_speed or jump_speed in the Inspector, or select the Moving Platform and change travel and period. Press ▶ Play to feel the difference; Stop brings everything back.",
                        "Selecione Player e mude run_speed ou jump_speed no Inspetor, ou selecione a Moving Platform e mude travel e period. Pressione ▶ Jogar para sentir a diferença; Parar traz tudo de volta."
                    ),
                    Focus::Tab(Tab::Inspector),
                ),
                step(
                    tr!("Make it yours", "Deixe com a sua cara"),
                    tr!(
                        "Duplicate a platform (Ctrl+D), drag it somewhere new, drop a few coins on it and hide a sixth Frost Shard: the Player script counts them by name when the game starts.",
                        "Duplique uma plataforma (Ctrl+D), arraste para outro lugar, espalhe algumas moedas e esconda um sexto Frost Shard: o script Player conta os shards pelo nome quando o jogo começa."
                    ),
                    Focus::Tab(Tab::Scene),
                ),
            ],
        },
        Tutorial {
            id: "export",
            title: tr!("Exporting your game", "Exportando seu jogo"),
            summary: tr!("Turn the project into something people can run.", "Transforme o projeto em algo que as pessoas possam rodar."),
            steps: vec![
                step(
                    tr!("Check the game camera", "Confira a câmera do jogo"),
                    tr!(
                        "The Game tab shows what players will see: the first Camera in the scene. The scene that is open when you export is the one the game starts in.",
                        "A aba Jogo mostra o que os jogadores vão ver: a primeira Câmera da cena. A cena aberta no momento da exportação é a que o jogo inicia."
                    ),
                    Focus::Tab(Tab::Game),
                ),
                step(
                    tr!("Export", "Exporte"),
                    tr!(
                        "Choose File > Export game… and pick a folder outside the project. Cryo creates a folder (an .app on macOS) with the executable and your project inside. It opens straight into the game, with no editor; Esc quits.",
                        "Escolha Arquivo > Exportar jogo… e selecione uma pasta fora do projeto. A Cryo cria uma pasta (um .app no macOS) com o executável e o seu projeto dentro. Ele abre direto no jogo, sem o editor; Esc fecha."
                    ),
                    Focus::MenuBar,
                ),
                step(
                    tr!("Share it", "Compartilhe"),
                    tr!(
                        "Zip that folder and send it. The export is for the system you are using now: export on Windows for Windows players, on macOS for Mac players, and so on.",
                        "Compacte essa pasta e envie. A exportação vale para o sistema que você está usando: exporte no Windows para quem joga no Windows, no macOS para quem usa Mac, e assim por diante."
                    ),
                    Focus::None,
                ),
            ],
        },
    ]
}
