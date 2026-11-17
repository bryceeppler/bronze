import sys
import tkinter

text = sys.argv[1]
root = tkinter.Tk()
root.withdraw()
root.selection_handle(lambda offset, length: text[int(offset):int(offset) + int(length)], selection="PRIMARY")
root.selection_own(selection="PRIMARY")
root.mainloop()
