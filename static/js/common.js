window.addEventListener("load", function() {
  const volslider = document.querySelector("#volume-slider");
  const playbutton = document.querySelector("#play-button"); // null if we are not on artist.html
  const setvolume = localStorage.getItem("volume") ?? 1;


  volslider.value = setvolume;
  if (playbutton !== null)
    playbutton.volume = Number(setvolume);

  volslider.addEventListener("change", () => {
    localStorage.setItem("volume", volslider.value);
    if (playbutton !== null)
      playbutton.volume = Number(volslider.value);
  });
});
